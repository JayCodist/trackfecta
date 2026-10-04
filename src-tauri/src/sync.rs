//! Background sync loop and session connection.
//!
//! How the loop stays inside the free-plan limit of about 30 requests per
//! hour. This limit is much lower than the documented limit of 1 request per
//! second:
//!   * Reads from the cache use no requests. The UI renders from SQLite, so
//!     looking at data never costs a request.
//!   * Background polling only spends a request when the rolling budget has
//!     room. `Budget::try_consume_background` reserves about 4 slots for
//!     user actions. Otherwise the loop sleeps until the oldest request ages
//!     out of the window (at most 1 hour).
//!   * The first refresh of a session is a full fetch. Later refreshes are
//!     delta fetches (`since =` the last change timestamp), so they stay
//!     small.
//!   * Local edits and deletes are queued in the cache with the dirty flag.
//!     The loop pushes one per tick, oldest first. User actions always get
//!     budget priority over polling.
//!   * When the window is full, the UI shows "sync paused, resumes in Xm"
//!     instead of sending more requests.
//!
//! The loop also recovers by itself. If a token is cached but no session
//! exists, it retries `connect`. This happens after a temporary network
//! failure at startup, or when auth drops.

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::state::{
    broadcast, emit_toast, mark_disconnected, start_of_window, window_days, AppState, ConnStatus,
    LastEntry, Session,
};
use crate::toggl::{TogglClient, TogglError};

pub fn spawn(app: AppHandle<Wry>) {
    tauri::async_runtime::spawn(async move {
        let wakeup = app.state::<AppState>().wakeup.clone();
        loop {
            tokio::select! {
                _ = tokio::time::sleep(next_sleep(&app)) => {}
                _ = wakeup.notified() => {}
            }
            tick(&app).await;
        }
    });
}

/// How long to wait before the next background attempt:
///   * A dirty row is waiting to push: soon. Pushes outrank polling.
///   * The budget is full: until the oldest request ages out.
///   * The timer is running: 60 seconds. Idle: 5 minutes. Both still depend
///     on the budget.
fn next_sleep(app: &AppHandle<Wry>) -> Duration {
    let st = app.state::<AppState>();
    let dirty = st
        .store
        .as_ref()
        .map(|s| !s.dirty_entries().is_empty())
        .unwrap_or(false);
    if dirty {
        // A queued push must not send requests while the window is full. It
        // waits for the refill like everything else.
        if !st.budget.has_headroom() {
            return st.budget.until_refill() + Duration::from_secs(2);
        }
        return Duration::from_secs(2);
    }
    if !st.budget.has_background_headroom() {
        return st.budget.until_refill() + Duration::from_secs(2);
    }
    if st.timer.lock().unwrap().running {
        Duration::from_secs(60)
    } else {
        Duration::from_secs(300)
    }
}

async fn tick(app: &AppHandle<Wry>) {
    let (has_session, cached_token, connecting) = {
        let st = app.state::<AppState>();
        let has_session = st.session.lock().unwrap().is_some();
        let cached_token = st.token.lock().unwrap().clone();
        let connecting = st.connecting.load(std::sync::atomic::Ordering::SeqCst);
        (has_session, cached_token, connecting)
    };

    if !has_session {
        // The startup connect runs at the same time as this loop. Without
        // this guard, a focus wakeup re-ran connect and every connect-time
        // request fired twice. That showed up as doubled log lines.
        // A quota block must stop reconnects too: GET /me is itself a
        // request. Retrying it on every focus wakeup while the server was
        // blocking kept the window full and the app stuck.
        let headroom = app.state::<AppState>().budget.has_headroom();
        if let Some(token) = cached_token.filter(|_| !connecting && headroom) {
            // Retry a connection that failed for a temporary reason, such as
            // being offline at startup.
            let _ = connect(app, token).await;
        }
        return;
    }

    // 1. Push queued local changes first. User actions outrank polling and
    //    always get budget priority.
    if push_dirty(app).await {
        // Progress was made. Come back for more, or sleep per next_sleep.
        return;
    }

    // 2. Opportunistic poll: only when the budget allows background spending.
    if !app.state::<AppState>().budget.try_consume_background() {
        // next_sleep already accounts for the refill delay.
        return;
    }
    match refresh(app).await {
        Ok(()) => {}
        Err(TogglError::Unauthorized) => {
            // The token was revoked or expired on the server. Stop using it.
            *app.state::<AppState>().token.lock().unwrap() = None;
            mark_disconnected(app);
        }
        Err(TogglError::RateLimited { retry_after }) => {
            // Toggl's own limit does not agree with our model. Honor it and
            // back off. Record the block so every path (poll, push, and
            // reconnect) pauses, not only this one.
            note_rate_limit(app, retry_after);
            tokio::time::sleep(Duration::from_secs(retry_after.max(30))).await;
        }
        Err(other) => emit_toast(app, &other.to_string()),
    }
}

/// Pushes up to one queued local change to the server. Returns true if a
/// push was attempted, whether it succeeded or its failure was handled. Then
/// the loop re-runs soon.
async fn push_dirty(app: &AppHandle<Wry>) -> bool {
    // Stop when the hourly window is full. Pushing anyway only gets 429
    // errors. Rows stay dirty until a slot frees.
    if !app.state::<AppState>().budget.has_headroom() {
        return false;
    }
    let (client, store) = {
        let st = app.state::<AppState>();
        let session = st.session.lock().unwrap();
        let client = match session.as_ref() {
            Some(Session { client, .. }) => client.clone(),
            None => return false,
        };
        (client, st.store.clone())
    };
    let Some(store) = store else {
        return false;
    };
    // One per tick also keeps us inside the limit of about 1 request per
    // second.
    let Some(dirty) = store.dirty_entries().into_iter().next() else {
        return false;
    };

    if dirty.deleted {
        return match client.delete_entry(dirty.workspace_id, dirty.id).await {
            Ok(()) => {
                crate::logger::log("info", &format!("push: DELETE {} ok", dirty.id));
                store.clear_dirty(dirty.id, Some(true));
                broadcast(app);
                true
            }
            Err(e) => handle_push_error(app, &store, &dirty, e).await,
        };
    }

    // v9 needs `duration` as an integer. It rejects a float such as 3600.0.
    // A running entry (stop=null) uses the Toggl convention duration = -start.
    let duration = dirty
        .stop
        .map(|s| (s - dirty.start).max(0))
        .unwrap_or(-dirty.start);
    let mut body = serde_json::json!({
        "workspace_id": dirty.workspace_id,
        "description": dirty.description,
        "start": iso(dirty.start),
        "stop": dirty.stop.map(iso),
        "duration": duration,
        "project_id": dirty.project_id,
        "tags": dirty.tags,
        "billable": dirty.billable,
        "duronly": false,
        "created_with": "TrackFecta",
    });
    if dirty.id > 0 && dirty.stop.is_none() {
        // This is an edit of a running entry on the server. Send only the
        // metadata. If stop and duration are present, the PUT route treats
        // them as the create convention, which it does not accept the same
        // way as POST.
        let map = body.as_object_mut().unwrap();
        map.remove("start");
        map.remove("stop");
        map.remove("duration");
        map.remove("duronly");
    }
    let push = if dirty.id < 0 {
        // A manual entry that was never on the server. POST it, then replace
        // the local row. The real server id replaces the negative temporary
        // id.
        client.create_entry(&body).await.map(|created| {
            crate::logger::log(
                "info",
                &format!("push: POST created server id {} ok", created.id),
            );
            store.remove_if_unsynced(dirty.id);
            store.upsert(&created);
            store.clear_dirty(created.id, None);
        })
    } else {
        // An edit of a synced entry. PUT the full time_entry and adopt the
        // response. Clear the dirty flag first. The upsert guard protects
        // pending local changes, and it must not block the server copy.
        client
            .update_entry(dirty.workspace_id, dirty.id, &body)
            .await
            .map(|updated| {
                crate::logger::log("info", &format!("push: PUT {} ok", updated.id));
                store.clear_dirty(updated.id, None);
                store.upsert(&updated);
            })
    };
    match push {
        Ok(()) => {
            broadcast(app);
            true
        }
        Err(e) => handle_push_error(app, &store, &dirty, e).await,
    }
}

/// The shared failure path for a queued push. A rate limit pauses the loop.
/// The row stays dirty and retries after the wait. A hard error drops the
/// row, so one bad entry cannot block the queue. Rows with a negative
/// temporary id are removed.
async fn handle_push_error(
    app: &AppHandle<Wry>,
    store: &crate::store::Store,
    dirty: &crate::store::DirtyEntry,
    e: TogglError,
) -> bool {
    match e {
        TogglError::Unauthorized => {
            crate::logger::log("warn", &format!("push {} rejected: unauthorized", dirty.id));
            *app.state::<AppState>().token.lock().unwrap() = None;
            mark_disconnected(app);
        }
        TogglError::RateLimited { retry_after } => {
            crate::logger::log(
                "warn",
                &format!("push {} rate-limited, retry in {retry_after}s", dirty.id),
            );
            note_rate_limit(app, retry_after);
            emit_toast(
                app,
                &format!("API limit reached. Retrying in {retry_after}s."),
            );
            tokio::time::sleep(Duration::from_secs(retry_after.max(30))).await;
        }
        other => {
            crate::logger::log(
                "error",
                &format!(
                    "push {} ({}) failed: {other}",
                    dirty.id,
                    if dirty.deleted { "DELETE" } else { "POST/PUT" }
                ),
            );
            if dirty.id < 0 {
                store.remove_if_unsynced(dirty.id);
            } else {
                store.clear_dirty(dirty.id, None);
            }
            emit_toast(app, &format!("Sync failed for an entry: {other}"));
            broadcast(app);
        }
    }
    true
}

fn iso(ts: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

/// Validates a token with `GET /me`, installs it as the active session, and
/// runs an initial refresh. Any failure leaves the app disconnected. A
/// rejected token also removes the cached copy, so the loop stops retrying
/// it.
pub async fn connect(app: &AppHandle<Wry>, token: String) -> Result<(), TogglError> {
    use std::sync::atomic::Ordering::SeqCst;
    // This guard stops the poll loop from starting a second connect. See
    // AppState.connecting.
    struct Guard(Arc<std::sync::atomic::AtomicBool>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.store(false, SeqCst);
        }
    }
    let st = app.state::<AppState>();
    if st.connecting.swap(true, SeqCst) {
        // A connect is already in progress.
        return Ok(());
    }
    let _guard = Guard(st.connecting.clone());
    st.timer.lock().unwrap().status = ConnStatus::Verifying;
    broadcast(app);

    let budget = st.budget.clone();
    let client = TogglClient::new(&token, budget);
    match client.me().await {
        Ok(user) => {
            *st.session.lock().unwrap() = Some(Session {
                client: Arc::new(client),
                user,
            });
            // A successful GET /me proves the quota window is open again.
            st.budget.clear_block();
            if let Some(store) = st.store.as_ref() {
                store.set_meta("quota_blocked_until", 0);
            }
            // Take the first snapshot right away. The full window plus the
            // reconcile step applies deletions made on other devices while
            // this app was offline.
            let _ = refresh_full(app).await;
            // Cache the reference data for the pickers and row labels:
            // projects, clients, and tags. This is a few requests per
            // session, at connect time only.
            fetch_reference_data(app).await;
            Ok(())
        }
        Err(e) => match e {
            TogglError::Unauthorized => {
                // The token was revoked or expired. Stop using it and show
                // the Auth screen.
                *st.token.lock().unwrap() = None;
                mark_disconnected(app);
                Err(e)
            }
            TogglError::RateLimited { retry_after } => {
                // The quota window is full. Honor the server's reset time.
                // Stay "verifying". The loop pauses reconnects until the
                // block lifts. See tick.
                note_rate_limit(app, retry_after);
                Err(e)
            }
            other => {
                // A network failure is temporary. Stay "verifying". The loop
                // retries with the cached token on the next tick.
                Err(other)
            }
        },
    }
}

/// Record a server quota limit (402 or 429). The budget blocks every
/// request until the reset time. The time is also stored in the cache, so
/// a restart does not forget the block and hammer the API again.
fn note_rate_limit(app: &AppHandle<Wry>, retry_after: u64) {
    let st = app.state::<AppState>();
    let d = Duration::from_secs(retry_after.max(30));
    st.budget.block_for(d);
    if let Some(store) = st.store.as_ref() {
        let until = chrono::Utc::now().timestamp() + d.as_secs() as i64;
        store.set_meta("quota_blocked_until", until);
    }
    broadcast(app);
}

/// One manual sync step, from the Sync-now button in Settings. Runs the
/// same two steps as a loop tick right now: push one queued change, or
/// poll once. Unlike background polling, a manual sync may spend the last
/// interactive slots of the window (it only needs `has_headroom`), but it
/// never sends anything during a server block.
pub async fn force_sync(app: &AppHandle<Wry>) -> Result<(), String> {
    let connected = app.state::<AppState>().session.lock().unwrap().is_some();
    if !connected {
        return Err("Not connected to Toggl.".into());
    }
    if !app.state::<AppState>().budget.has_headroom() {
        return Err("API quota reached. Wait for a slot to free up.".into());
    }
    // Queued local changes outrank a poll, same as the loop.
    if push_dirty(app).await {
        return Ok(());
    }
    match refresh(app).await {
        Ok(()) => Ok(()),
        Err(TogglError::Unauthorized) => {
            *app.state::<AppState>().token.lock().unwrap() = None;
            mark_disconnected(app);
            Err("Token rejected. Please enter it again.".into())
        }
        Err(TogglError::RateLimited { retry_after }) => {
            note_rate_limit(app, retry_after);
            Err(format!("API limit reached. Retry in {retry_after}s."))
        }
        Err(other) => Err(other.to_string()),
    }
}

/// Pulls entries changed since the last sync, or a full page on the first
/// run. Upserts them into the cache and reconciles the timer snapshot.
/// `full` fetches Toggl's default range of about 10 days. It also marks
/// cached rows as deleted when the response does not have them. Those are
/// deletions made on the web or another device while this app was not
/// polling. The delta feed alone can miss them.
pub async fn refresh(app: &AppHandle<Wry>) -> Result<(), TogglError> {
    refresh_inner(app, false).await
}

/// Full-range refresh with reconcile. Used on connect, once per session.
pub async fn refresh_full(app: &AppHandle<Wry>) -> Result<(), TogglError> {
    refresh_inner(app, true).await
}

async fn refresh_inner(app: &AppHandle<Wry>, full: bool) -> Result<(), TogglError> {
    let (client, workspace_id) = {
        let st = app.state::<AppState>();
        let guard = st.session.lock().unwrap();
        match guard.as_ref() {
            Some(Session { client, user }) => (client.clone(), user.default_workspace_id),
            None => return Ok(()),
        }
    };
    let (since, store, days) = {
        let st = app.state::<AppState>();
        let days = window_days(st.store.as_ref());
        let since = if full {
            // Cover the full history window (one month by default). `since`
            // filters by last-modified time. An entry's modification is never
            // older than its start, so every entry inside the window comes
            // back. The reconcile step below depends on this.
            Some(start_of_window(days))
        } else {
            st.store.as_ref().and_then(|s| s.get_meta("last_sync"))
        };
        (since, st.store.clone(), days)
    };

    // `since = None` gets Toggl's default range of about 10 days. After the
    // first transfer, delta polls bring only what changed. That is cheap in
    // bytes and in budget.
    let entries: Vec<crate::toggl::TimeEntry> = client.me_time_entries(workspace_id, since).await?;
    crate::logger::log(
        "info",
        &format!("sync: {} entries (since={since:?})", entries.len()),
    );

    let now_ts = chrono::Utc::now().timestamp();
    let cursor = entries
        .iter()
        .filter_map(|e| e.updated_at.map(|t| t.timestamp()))
        .max()
        .or(since)
        .unwrap_or(now_ts)
        .min(now_ts);

    if let Some(store) = &store {
        for e in &entries {
            // A server-side deletion arrives as a full row with
            // `server_deleted_at` set. A live probe on 2026-10-03 showed the
            // `{deleted:true}` shape never actually occurs. An upsert of a
            // deleted row would keep the stale row alive forever, so deleted
            // rows go to the tombstone handler instead.
            if e.is_deleted() {
                store.apply_tombstone(e.id);
            } else {
                store.upsert(e);
            }
        }
        if full {
            // The response is the authoritative copy of the window. Any row
            // we still hold inside it that is not in the response was deleted
            // somewhere else (web or another device) while we were not
            // looking.
            let present: Vec<i64> = entries
                .iter()
                .filter(|e| !e.is_deleted())
                .map(|e| e.id)
                .collect();
            let removed = store.reconcile_absent(workspace_id, start_of_window(days), &present);
            if removed > 0 {
                crate::logger::log(
                    "info",
                    &format!("reconcile: {removed} cached entries gone server-side"),
                );
            }
        }
        store.set_meta("last_sync", cursor);
        store.purge_tombstones();
    }

    let running_entry = entries.iter().find(|e| e.stop.is_none() && !e.is_deleted());
    // A delta poll stops including the running entry once the cursor passes
    // its last update. Check the open row in the cache before declaring "not
    // running".
    let cached_open = if running_entry.is_none() {
        store.as_ref().and_then(|s| s.open_entry())
    } else {
        None
    };

    {
        let st = app.state::<AppState>();
        let mut t = st.timer.lock().unwrap();
        t.status = ConnStatus::Connected;
        // Without a cache, sum the live response instead. With a cache,
        // `broadcast` recomputes the total from cached rows. That uses no
        // requests.
        if store.is_none() {
            t.today_seconds = entries
                .iter()
                .filter(|e| e.stop.is_some() && !e.deleted.unwrap_or(false))
                .map(|e| e.duration.max(0.0) as i64)
                .sum();
        }
        if let Some(e) = running_entry {
            t.running = true;
            t.description = e.description.clone();
            t.started_at = Some(e.start.timestamp().to_string());
            *st.entry_id.lock().unwrap() = Some(e.id);
            if e.description.is_some() || e.project_id.is_some() {
                *st.last_entry.lock().unwrap() = Some(LastEntry {
                    description: e.description.clone(),
                    project_id: e.project_id,
                    tags: e.tags.clone().unwrap_or_default(),
                    billable: e.billable.unwrap_or(false),
                });
            }
        } else if let Some(row) = cached_open {
            t.running = true;
            t.description = row.description.clone();
            t.started_at = Some(row.start.to_string());
            *st.entry_id.lock().unwrap() = Some(row.id);
            if row.description.is_some() || row.project_id.is_some() {
                *st.last_entry.lock().unwrap() = Some(LastEntry {
                    description: row.description.clone(),
                    project_id: row.project_id,
                    tags: row.tags.clone(),
                    billable: row.billable,
                });
            }
        } else {
            t.running = false;
            t.description = None;
            t.started_at = None;
            *st.entry_id.lock().unwrap() = None;
        }
    }
    broadcast(app);
    Ok(())
}

/// Caches the workspace project, client, and tag lists. Then every entry row
/// can show its project, with the Toggl color, and its client, and the
/// pickers offer the full lists. One request each, at connect time only.
async fn fetch_reference_data(app: &AppHandle<Wry>) {
    let (client, store, workspace_id) = {
        let st = app.state::<AppState>();
        let guard = st.session.lock().unwrap();
        let Some(Session { client, user }) = guard.as_ref() else {
            return;
        };
        (client.clone(), st.store.clone(), user.default_workspace_id)
    };
    let Some(store) = store else { return };
    let mut changed = false;
    match client.workspace_projects(workspace_id).await {
        Ok(projects) => {
            let keep: Vec<i64> = projects.iter().map(|p| p.id).collect();
            let stale = store.project_ids_except(&keep);
            store.replace_projects(&projects);
            if !stale.is_empty() {
                store.remove_projects(&stale);
            }
            crate::logger::log("info", &format!("projects: cached {}", projects.len()));
            changed = true;
        }
        Err(e) => crate::logger::log("warn", &format!("projects fetch failed: {e}")),
    }
    match client.workspace_clients(workspace_id).await {
        Ok(clients) => {
            store.replace_clients(&clients);
            crate::logger::log("info", &format!("clients: cached {}", clients.len()));
            changed = true;
        }
        Err(e) => crate::logger::log("warn", &format!("clients fetch failed: {e}")),
    }
    match client.workspace_tags(workspace_id).await {
        Ok(tags) => {
            store.replace_tags(&tags);
            crate::logger::log("info", &format!("tags: cached {}", tags.len()));
            changed = true;
        }
        Err(e) => crate::logger::log("warn", &format!("tags fetch failed: {e}")),
    }
    if changed {
        let _ = app.emit("pickers-changed", ());
        broadcast(app);
    }
}
