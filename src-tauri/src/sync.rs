//! Background polling loop + session connection.
//!
//! Toggl has no public websocket and a ~1 req/s rate limit, so we poll:
//!   - while a timer is running: every 10s
//!   - otherwise: every 30s
//!   - `AppState::wakeup` fires an immediate tick on focus / start / stop / re-auth.
//!
//! The loop also self-heals: if a token is cached but no session exists
//! (transient network failure at startup, dropped auth), it retries `connect`.

use std::sync::Arc;
use std::time::Duration;

use chrono::{Duration as ChronoDur, Utc};
use tauri::{AppHandle, Manager, Wry};

use crate::state::{broadcast, emit_toast, mark_disconnected, AppState, ConnStatus, Session};
use crate::toggl::{TogglClient, TogglError};

pub fn spawn(app: AppHandle<Wry>) {
    tauri::async_runtime::spawn(async move {
        let wakeup = app.state::<AppState>().wakeup.clone();
        loop {
            let interval = poll_interval(&app);
            tokio::select! {
                _ = tokio::time::sleep(interval) => {}
                _ = wakeup.notified() => {}
            }
            tick(&app).await;
        }
    });
}

fn poll_interval(app: &AppHandle<Wry>) -> Duration {
    let running = app.state::<AppState>().timer.lock().unwrap().running;
    if running {
        Duration::from_secs(10)
    } else {
        Duration::from_secs(30)
    }
}

async fn tick(app: &AppHandle<Wry>) {
    let (has_session, cached_token) = {
        let st = app.state::<AppState>();
        let has_session = st.session.lock().unwrap().is_some();
        let cached_token = st.token.lock().unwrap().clone();
        (has_session, cached_token)
    };

    if has_session {
        match refresh(app).await {
            Ok(()) => {}
            Err(TogglError::Unauthorized) => {
                // Token revoked/expired server-side: stop retrying with it.
                *app.state::<AppState>().token.lock().unwrap() = None;
                mark_disconnected(app);
            }
            Err(TogglError::RateLimited { retry_after }) => {
                // Honor Retry-After before the next poll.
                tokio::time::sleep(Duration::from_secs(retry_after.max(2))).await;
            }
            Err(other) => emit_toast(app, &other.to_string()),
        }
    } else if let Some(token) = cached_token {
        // Retry a connection that failed transiently (e.g. offline at startup).
        let _ = connect(app, token).await;
    }
}

/// Validate a token via `GET /me`, install it as the active session, and run an
/// initial refresh. Any failure leaves us disconnected; a rejected token also
/// drops the cached copy so the loop stops retrying it.
pub async fn connect(app: &AppHandle<Wry>, token: String) -> Result<(), TogglError> {
    {
        let st = app.state::<AppState>();
        st.timer.lock().unwrap().status = ConnStatus::Verifying;
    }
    broadcast(app);

    let client = TogglClient::new(&token);
    match client.me().await {
        Ok(user) => {
            {
                let st = app.state::<AppState>();
                *st.session.lock().unwrap() = Some(Session {
                    client: Arc::new(client),
                    user,
                });
            }
            // First snapshot right away; failures handled by the loop later.
            let _ = refresh(app).await;
            Ok(())
        }
        Err(e) => {
            if matches!(e, TogglError::Unauthorized) {
                // Revoked/expired token: stop retrying, show the Auth screen.
                *app.state::<AppState>().token.lock().unwrap() = None;
                mark_disconnected(app);
            }
            // Transient (network): stay "verifying" — the loop retries with the
            // cached token on the next tick.
            Err(e)
        }
    }
}

/// Pull today's entries and reconcile the timer snapshot. No-op without a session.
pub async fn refresh(app: &AppHandle<Wry>) -> Result<(), TogglError> {
    let (client, _workspace_id) = {
        let st = app.state::<AppState>();
        let guard = st.session.lock().unwrap();
        match guard.as_ref() {
            Some(Session { client, user }) => (client.clone(), user.default_workspace_id),
            None => return Ok(()),
        }
    };

    let now = Utc::now();
    let start_of_day = now.date_naive().and_time(chrono::NaiveTime::MIN).and_utc();
    let entries = client
        .time_entries(start_of_day, now + ChronoDur::minutes(1))
        .await?;

    let running_entry = entries.iter().find(|e| e.stop.is_none());
    let today_done: i64 = entries
        .iter()
        .filter(|e| e.stop.is_some())
        .map(|e| e.duration.max(0.0) as i64)
        .sum();

    {
        let st = app.state::<AppState>();
        let mut t = st.timer.lock().unwrap();
        t.status = ConnStatus::Connected;
        t.today_seconds = today_done;
        match running_entry {
            Some(e) => {
                t.running = true;
                t.description = e.description.clone();
                t.started_at = Some(e.start.timestamp().to_string());
                *st.entry_id.lock().unwrap() = Some(e.id);
                if e.description.is_some() {
                    *st.last_description.lock().unwrap() = e.description.clone();
                }
            }
            None => {
                t.running = false;
                t.description = None;
                t.started_at = None;
                *st.entry_id.lock().unwrap() = None;
            }
        }
    }
    broadcast(app);
    Ok(())
}
