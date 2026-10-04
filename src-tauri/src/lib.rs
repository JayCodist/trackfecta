mod budget;
mod logger;
mod secrets;
mod state;
mod store;
mod sync;
mod toggl;
mod tray;

use std::sync::Arc;

use tauri::{
    menu::{MenuBuilder, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, WebviewWindow, Wry,
};

use state::{broadcast, mark_disconnected, AppState, LastEntry, Session, TimerState, TrayItems};
use store::{EntryRow, PickerProject};

// ---------- commands ----------

/// Gives the UI the current state when it starts.
/// The UI calls this on mount. The first timer-state broadcast can happen
/// before the webview starts listening. This command covers that gap.
#[tauri::command]
fn get_state(state: tauri::State<'_, AppState>) -> TimerState {
    state.timer.lock().unwrap().clone()
}

#[tauri::command]
async fn set_api_token(app: tauri::AppHandle<Wry>, token: String) -> Result<(), String> {
    let token = token.trim().to_string();
    secrets::set_token(&token)?;
    *app.state::<AppState>().token.lock().unwrap() = Some(token.clone());
    sync::connect(&app, token).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn start_timer(
    app: tauri::AppHandle<Wry>,
    description: String,
    project_id: Option<i64>,
    tags: Vec<String>,
    billable: bool,
) -> Result<(), String> {
    do_start(
        &app,
        StartPayload {
            description,
            project_id,
            tags,
            billable,
        },
    )
}

#[tauri::command]
fn stop_timer(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    do_stop(&app)
}

#[tauri::command]
fn resume_last(app: tauri::AppHandle<Wry>) {
    request_resume(&app);
}

/// Removes the token from the keyring and from memory, then shows the Auth
/// screen.
#[tauri::command]
fn logout(app: tauri::AppHandle<Wry>) {
    secrets::delete_token();
    {
        let st = app.state::<AppState>();
        *st.token.lock().unwrap() = None;
        *st.last_entry.lock().unwrap() = None;
    }
    mark_disconnected(&app);
}

/// Gives the cached entries for the history window. This uses no API
/// requests. The UI groups them by day: Today, Yesterday, and dates.
#[tauri::command]
fn get_entries(state: tauri::State<'_, AppState>) -> Vec<EntryRow> {
    state
        .store
        .as_ref()
        .map(|s| s.entries_since(state::start_of_window(state::window_days(Some(s)))))
        .unwrap_or_default()
}

/// Grows the history window for the "Load earlier entries" button. This
/// doubles the window, up to one year. Then it wakes the sync loop to fetch
/// the wider range.
#[tauri::command]
fn extend_window(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    let st = app.state::<AppState>();
    let Some(store) = st.store.as_ref() else {
        return Err("cache unavailable".into());
    };
    let next = (state::window_days(Some(store)) * 2).min(366);
    store.set_setting("window_days", &next.to_string());
    // Put the delta cursor back to the start of the new window. The wider
    // fetch re-transfers the older rows. Upsert of a row already in the
    // cache does not change it.
    let start = state::start_of_window(next);
    store.set_meta("last_sync", start);
    drop(st);
    broadcast(&app);
    app.state::<AppState>().wakeup.notify_one();
    Ok(())
}

/// Gives the cached workspace projects, with colors and client names. This
/// uses no API requests.
#[tauri::command]
fn get_picker_projects(state: tauri::State<'_, AppState>) -> Vec<PickerProject> {
    state
        .store
        .as_ref()
        .map(|s| s.picker_projects())
        .unwrap_or_default()
}

#[tauri::command]
fn get_picker_tags(state: tauri::State<'_, AppState>) -> Vec<String> {
    state
        .store
        .as_ref()
        .map(|s| s.picker_tags())
        .unwrap_or_default()
}

/// Gives the recent log lines for the Diagnostics section in Settings.
/// Include these lines in a bug report. Also gives the log file path.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Diagnostics {
    log_tail: String,
    log_path: String,
}

#[tauri::command]
fn get_diagnostics() -> Diagnostics {
    Diagnostics {
        log_tail: logger::read_tail(8000),
        log_path: logger::file_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    }
}

/// Gives the user settings. All values are local. This uses no API requests.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    hourly_cap: usize,
    default_project_id: Option<i64>,
    default_project_name: Option<String>,
    /// One of: system, light, or dark. The webview applies this to the
    /// html element.
    theme: String,
    /// Whether the tray time readout includes seconds.
    tray_show_seconds: bool,
}

#[tauri::command]
fn get_settings(app: tauri::AppHandle<Wry>) -> AppSettings {
    let st = app.state::<AppState>();
    let default_project_id = st
        .store
        .as_ref()
        .and_then(|s| s.get_setting("default_project"))
        .and_then(|v| v.parse::<i64>().ok());
    let default_project_name = default_project_id.and_then(|id| {
        st.store
            .as_ref()
            .and_then(|s| s.picker_projects().into_iter().find(|p| p.id == id))
            .map(|p| p.name)
    });
    let theme = st
        .store
        .as_ref()
        .and_then(|s| s.get_setting("theme"))
        .unwrap_or_else(|| "system".into());
    let tray_show_seconds = st
        .store
        .as_ref()
        .and_then(|s| s.get_setting("tray_show_seconds"))
        .map(|v| v == "true")
        .unwrap_or(false);
    AppSettings {
        hourly_cap: st.budget.max(),
        default_project_id,
        default_project_name,
        theme,
        tray_show_seconds,
    }
}

/// Saves a setting. The hourly_cap setting takes effect at once. The
/// default_project setting prefills the project picker of the timer. A null
/// value clears the setting.
/// The value is serde_json::Value because a Svelte number input sends a JSON
/// number. An Option<String> parameter rejected those values, and the whole
/// save failed.
#[tauri::command]
fn set_setting(
    app: tauri::AppHandle<Wry>,
    key: String,
    value: Option<serde_json::Value>,
) -> Result<(), String> {
    let st = app.state::<AppState>();
    let value = match value {
        Some(serde_json::Value::String(s)) => Some(s),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => Some(other.to_string()),
    };
    if key == "hourly_cap" {
        let cap: usize = value
            .as_deref()
            .unwrap_or("")
            .parse()
            .map_err(|_| "hourly_cap must be a number".to_string())?;
        st.budget.set_max(cap);
    }
    if let Some(store) = st.store.as_ref() {
        match &value {
            Some(v) => store.set_setting(&key, v),
            None => store.clear_setting(&key),
        }
    }
    drop(st);
    if key == "tray_show_seconds" {
        // Draw the tray icon again with the new format.
        tray::refresh(&app);
    }
    // The requests-left number can change with the new cap.
    broadcast(&app);
    Ok(())
}

/// Edits an entry. Writes the change to the cache at once and puts the push
/// in the queue.
#[tauri::command]
fn update_entry(
    app: tauri::AppHandle<Wry>,
    id: i64,
    description: String,
    start: i64,
    stop: Option<i64>,
    tags: Vec<String>,
    project_id: Option<i64>,
    billable: bool,
) -> Result<(), String> {
    let st = app.state::<AppState>();
    if let Some(store) = st.store.as_ref() {
        let desc = if description.trim().is_empty() {
            None
        } else {
            Some(description.as_str())
        };
        store.update_local(id, desc, &tags, start, stop, project_id, billable);
        // The tray tooltip and the timer bar read timer.description, not the
        // cached row. Keep that field current for the running entry.
        if stop.is_none() {
            let mut t = st.timer.lock().unwrap();
            if t.running {
                t.description = desc.map(str::to_string);
            }
        }
    }
    broadcast(&app);
    st.wakeup.notify_one();
    Ok(())
}

/// Deletes an entry. Marks the row deleted in the cache at once and puts
/// the push in the queue.
#[tauri::command]
fn delete_entry(app: tauri::AppHandle<Wry>, id: i64) -> Result<(), String> {
    let st = app.state::<AppState>();
    if let Some(store) = st.store.as_ref() {
        store.mark_deleted(id);
    }
    broadcast(&app);
    st.wakeup.notify_one();
    Ok(())
}

/// Creates a manual entry. Inserts it in the cache with a negative temporary
/// id and puts the push in the queue. When the push succeeds, the real
/// server id replaces the temporary id.
#[tauri::command]
fn create_entry(
    app: tauri::AppHandle<Wry>,
    description: String,
    start: i64,
    stop: Option<i64>,
    tags: Vec<String>,
    project_id: Option<i64>,
    billable: bool,
) -> Result<(), String> {
    let st = app.state::<AppState>();
    let workspace_id = {
        let guard = st.session.lock().unwrap();
        guard
            .as_ref()
            .map(|s| s.user.default_workspace_id)
            .ok_or("not connected")?
    };
    let temp_id = -chrono::Utc::now().timestamp_micros();
    if let Some(store) = st.store.as_ref() {
        let entry = toggl::TimeEntry {
            id: temp_id,
            workspace_id,
            description: (!description.trim().is_empty()).then(|| description.clone()),
            start: chrono::DateTime::<chrono::Utc>::from_timestamp(start, 0)
                .unwrap_or_else(chrono::Utc::now),
            stop: stop.and_then(|s| chrono::DateTime::<chrono::Utc>::from_timestamp(s, 0)),
            duration: stop.map(|s| (s - start) as f64).unwrap_or(0.0),
            project_id,
            tags: Some(tags),
            billable: Some(billable),
            updated_at: None,
            deleted: None,
            server_deleted_at: None,
        };
        store.insert_manual(&entry);
    }
    broadcast(&app);
    st.wakeup.notify_one();
    Ok(())
}

// ---------- core operations (shared by the tray and the commands) ----------

/// The data the user submitted in the timer bar, or the data reused by
/// "Resume last entry".
#[derive(Clone)]
struct StartPayload {
    description: String,
    project_id: Option<i64>,
    tags: Vec<String>,
    billable: bool,
}

/// Starts a timer. Like every other action, this is optimistic. The running
/// entry goes into the cache at once with a negative temporary id and the
/// dirty flag. The UI updates at once. The sync loop POSTs the entry and
/// replaces the temporary id with the server id. If the push fails, the
/// cache removes the row and the timer stops.
fn do_start(app: &AppHandle<Wry>, payload: StartPayload) -> Result<(), String> {
    let (workspace_id, store) = {
        let st = app.state::<AppState>();
        if st.timer.lock().unwrap().running {
            return Err("a timer is already running".into());
        }
        let workspace_id = {
            let guard = st.session.lock().unwrap();
            let Some(Session { user, .. }) = guard.as_ref() else {
                return Err("not connected".into());
            };
            user.default_workspace_id
        };
        (workspace_id, st.store.clone())
    };

    let now = chrono::Utc::now();
    let desc = (!payload.description.trim().is_empty()).then(|| payload.description.clone());
    let temp_id = -now.timestamp_micros();
    let entry = toggl::TimeEntry {
        id: temp_id,
        workspace_id,
        description: desc.clone(),
        start: now,
        stop: None,
        duration: 0.0,
        project_id: payload.project_id,
        tags: Some(payload.tags.clone()),
        billable: Some(payload.billable),
        updated_at: None,
        deleted: None,
        server_deleted_at: None,
    };
    if let Some(store) = &store {
        // dirty=1 makes the push loop POST the row. stop:null means running.
        store.insert_manual(&entry);
    }
    {
        let st = app.state::<AppState>();
        let mut t = st.timer.lock().unwrap();
        t.running = true;
        t.description = desc.clone();
        t.started_at = Some(now.timestamp().to_string());
        *st.last_entry.lock().unwrap() = Some(LastEntry {
            description: desc,
            project_id: payload.project_id,
            tags: payload.tags,
            billable: payload.billable,
        });
        // The server id is unknown until the POST succeeds. do_stop finds
        // the running row with the open-entry lookup in the cache.
        *st.entry_id.lock().unwrap() = None;
    }
    broadcast(app);
    app.state::<AppState>().wakeup.notify_one();
    Ok(())
}

/// Stops the running timer. The cached row gets its stop time and duration
/// at once, with the dirty flag. The UI clears at once. The sync loop PUTs
/// the change. If the push fails, the timer goes back to running.
fn do_stop(app: &AppHandle<Wry>) -> Result<(), String> {
    let st = app.state::<AppState>();
    let entry_id = match *st.entry_id.lock().unwrap() {
        Some(id) => id,
        // The entry is not on the server yet. The running row is in the
        // cache with its temporary id.
        None => st
            .store
            .as_ref()
            .and_then(|s| s.open_entry())
            .map(|r| r.id)
            .ok_or("no running entry")?,
    };
    let now = chrono::Utc::now().timestamp();
    if let Some(store) = st.store.as_ref() {
        // stop_local computes the duration from the stored start time.
        store.stop_local(entry_id, now);
    }
    {
        let mut t = st.timer.lock().unwrap();
        t.running = false;
        t.description = None;
        t.started_at = None;
        *st.entry_id.lock().unwrap() = None;
    }
    drop(st);
    broadcast(app);
    app.state::<AppState>().wakeup.notify_one();
    Ok(())
}

/// Shows the window and asks the UI to prefill a new entry from the last
/// entry: description, project, and tags.
fn request_resume(app: &AppHandle<Wry>) {
    let last = app
        .state::<AppState>()
        .last_entry
        .lock()
        .unwrap()
        .clone();
    if let Some(w) = app.get_webview_window("main") {
        show_window(&w);
    }
    let _ = app.emit("resume-requested", last);
}

fn show_window(window: &WebviewWindow) {
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch shows the existing window instead.
            if let Some(w) = app.get_webview_window("main") {
                show_window(&w);
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            // These arguments go into the autostart .desktop entry. The app
            // starts minimized.
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            // ---- Start file logging first. Everything after this is logged. ----
            logger::install_panic_hook();
            if let Ok(dir) = app.path().app_data_dir() {
                logger::init(&dir);
            }
            // ---- Open the SQLite cache. If it fails, the app uses live data. ----
            let store = app
                .path()
                .app_data_dir()
                .ok()
                .and_then(|dir| store::Store::open(&dir))
                .map(Arc::new);
            app.manage(AppState::new(store));
            // Apply the hourly cap from settings. Paid plans allow more than 30.
            if let Some(cap) = app
                .state::<AppState>()
                .store
                .as_ref()
                .and_then(|s| s.get_setting("hourly_cap"))
                .and_then(|v| v.parse::<usize>().ok())
            {
                app.state::<AppState>().budget.set_max(cap);
            }

            // ---- Tray menu: status item with changing text, and actions ----
            // Linux trays do not get mouse events. All operations are in the menu.
            let status = MenuItem::with_id(app, "status", "Today: 0:00", true, None::<&str>)?;
            let show = MenuItem::with_id(app, "show", "Show ToggLinux", true, None::<&str>)?;
            let resume =
                MenuItem::with_id(app, "resume", "Resume last entry", false, None::<&str>)?;
            let stop = MenuItem::with_id(app, "stop", "Stop timer", false, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = MenuBuilder::new(app)
                .item(&status)
                .separator()
                .item(&show)
                .item(&resume)
                .item(&stop)
                .separator()
                .item(&quit)
                .build()?;

            let _tray = TrayIconBuilder::with_id("main-tray")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .tooltip("ToggLinux: unofficial Toggl Track client")
                .on_menu_event(|app, event| match event.id().0.as_str() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            show_window(&w);
                        }
                    }
                    "stop" => {
                        let _ = do_stop(app);
                    }
                    "resume" => request_resume(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            app.manage(TrayItems {
                status,
                show,
                resume,
                stop,
                quit,
            });

            // The tray icon shows the time tracked today and the running
            // state. Draw it once now, before the first broadcast. Then start
            // the ticker that keeps the running time counting. The ticker
            // uses no API requests.
            tray::refresh(app.handle());
            tray::spawn(app.handle().clone());

            // ---- Startup: restore the session if a token is in the keyring ----
            // A temporary failure, such as being offline at startup, leaves the
            // status as verifying. The sync loop retries connect with the
            // cached token by itself.
            if let Some(token) = secrets::get_token() {
                *app.state::<AppState>().token.lock().unwrap() = Some(token.clone());
                let app_clone = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let _ = sync::connect(&app_clone, token).await;
                });
            } else {
                mark_disconnected(app.handle());
            }

            // ---- background polling loop ----
            sync::spawn(app.handle().clone());

            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window hides it. The app keeps running in the tray.
            // The Quit item in the tray menu is the only way to exit.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
            // Wake the sync loop when the window gets focus. This keeps the
            // data fresh.
            if let tauri::WindowEvent::Focused(true) = event {
                window.state::<AppState>().wakeup.notify_one();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            set_api_token,
            logout,
            start_timer,
            stop_timer,
            resume_last,
            get_entries,
            extend_window,
            get_picker_projects,
            get_picker_tags,
            get_settings,
            set_setting,
            update_entry,
            delete_entry,
            create_entry,
            get_diagnostics
        ])
        .run(tauri::generate_context!())
        .expect("error while running ToggLinux");
}
