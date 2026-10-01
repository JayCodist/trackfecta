mod secrets;
mod state;
mod sync;
mod toggl;

use tauri::{
    menu::{MenuBuilder, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, WebviewWindow, Wry,
};

use state::{broadcast, mark_disconnected, AppState, Session, TimerState, TrayItems};
use toggl::TogglError;

// ---------- commands ----------

/// Current snapshot for the UI to fetch on mount (avoids a startup race where
/// the first `timer-state` broadcast fires before the webview is listening).
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
async fn start_timer(app: tauri::AppHandle<Wry>, description: String) -> Result<(), String> {
    do_start(&app, description).await
}

#[tauri::command]
async fn stop_timer(app: tauri::AppHandle<Wry>) -> Result<(), String> {
    do_stop(&app).await
}

#[tauri::command]
fn resume_last(app: tauri::AppHandle<Wry>) {
    request_resume(&app);
}

// ---------- core operations (take &AppHandle so tray + commands share them) ----------

/// Start a running entry via the API and update local state.
async fn do_start(app: &AppHandle<Wry>, description: String) -> Result<(), String> {
    let (client, workspace_id) = {
        let st = app.state::<AppState>();
        let guard = st.session.lock().unwrap();
        let Session { client, user } = guard.as_ref().ok_or("not connected")?;
        (client.clone(), user.default_workspace_id)
    };

    let desc = if description.trim().is_empty() {
        None
    } else {
        Some(description.as_str())
    };
    match client.start_entry(workspace_id, desc).await {
        Ok(entry) => {
            {
                let st = app.state::<AppState>();
                let mut t = st.timer.lock().unwrap();
                t.running = true;
                t.description = entry.description.clone();
                t.started_at = Some(entry.start.timestamp().to_string());
                *st.last_description.lock().unwrap() = entry.description.clone();
                *st.entry_id.lock().unwrap() = Some(entry.id);
            }
            broadcast(app);
            app.state::<AppState>().wakeup.notify_one();
            Ok(())
        }
        Err(TogglError::Unauthorized) => {
            mark_disconnected(app);
            Err("token rejected".into())
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Stop the running entry via the API and update local state.
async fn do_stop(app: &AppHandle<Wry>) -> Result<(), String> {
    let target = {
        let st = app.state::<AppState>();
        let entry_id = *st.entry_id.lock().unwrap();
        let guard = st.session.lock().unwrap();
        match (guard.as_ref(), entry_id) {
            (Some(Session { client, user }), Some(id)) => {
                Some((client.clone(), user.default_workspace_id, id))
            }
            _ => None,
        }
    };
    let Some((client, workspace_id, entry_id)) = target else {
        return Err("not connected".into());
    };

    match client.stop_entry(workspace_id, entry_id).await {
        Ok(_) => {
            {
                let st = app.state::<AppState>();
                let mut t = st.timer.lock().unwrap();
                t.running = false;
                t.description = None;
                t.started_at = None;
                *st.entry_id.lock().unwrap() = None;
            }
            broadcast(app);
            app.state::<AppState>().wakeup.notify_one();
            Ok(())
        }
        Err(TogglError::Unauthorized) => {
            mark_disconnected(app);
            Err("token rejected".into())
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Show the window and ask the UI to prefill a new entry from the last description.
fn request_resume(app: &AppHandle<Wry>) {
    let desc = app
        .state::<AppState>()
        .last_description
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default();
    if let Some(w) = app.get_webview_window("main") {
        show_window(&w);
    }
    let _ = app.emit("resume-requested", desc);
}

fn show_window(window: &WebviewWindow) {
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // Second launch -> surface the existing window instead
            if let Some(w) = app.get_webview_window("main") {
                show_window(&w);
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            // launch args written into the autostart .desktop entry -> start minimized
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState::new())
        .setup(|app| {
            // ---- tray menu: dynamic status item + actions ----
            // Linux trays get no mouse events, so every operation lives in the menu.
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
                .tooltip("ToggLinux — unofficial Toggl Track client")
                .on_menu_event(|app, event| match event.id().0.as_str() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            show_window(&w);
                        }
                    }
                    "stop" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = do_stop(&app).await;
                        });
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

            // ---- startup: restore a session if a token is cached ----
            // Transient failures (offline at boot) leave status=verifying and the
            // sync loop retries `connect` with the cached token automatically.
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
            // Daemon-like behavior: closing the window hides it; the app keeps
            // running in the tray. Real exit happens via tray -> Quit.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
            // Refresh when the window gains focus (cheap, keeps data fresh).
            if let tauri::WindowEvent::Focused(true) = event {
                window.state::<AppState>().wakeup.notify_one();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            set_api_token,
            start_timer,
            stop_timer,
            resume_last
        ])
        .run(tauri::generate_context!())
        .expect("error while running ToggLinux");
}
