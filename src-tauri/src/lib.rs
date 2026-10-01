use std::sync::Mutex;

use serde::Serialize;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, Runtime, WebviewWindow, Wry,
};

const KEYRING_SERVICE: &str = "com.togglinux.app";
const KEYRING_USER: &str = "toggl_api_token";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TimerState {
    running: bool,
    description: Option<String>,
    started_at: Option<String>,
    today_seconds: i64,
}

struct AppState {
    timer: Mutex<TimerState>,
}

// ---------- commands ----------

#[tauri::command]
fn has_api_token() -> bool {
    get_token().is_some()
}

#[tauri::command]
fn set_api_token(token: String) -> Result<(), String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| e.to_string())?
        .set_password(&token)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn start_timer(app: tauri::AppHandle<Wry>, state: tauri::State<'_, AppState>, description: String) -> Result<(), String> {
    // TODO(M1): POST to Toggl API. Placeholder state update + broadcast.
    {
        let mut t = state.timer.lock().unwrap();
        t.running = true;
        t.description = Some(description);
        t.started_at = Some(chrono_now_iso());
    }
    broadcast(&app, &state);
    Ok(())
}

#[tauri::command]
fn stop_timer(app: tauri::AppHandle<Wry>, state: tauri::State<'_, AppState>) -> Result<(), String> {
    // TODO(M1): PUT .../stop on Toggl API.
    {
        let mut t = state.timer.lock().unwrap();
        t.running = false;
        t.started_at = None;
    }
    broadcast(&app, &state);
    Ok(())
}

fn get_token() -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .ok()?
        .get_password()
        .ok()
}

fn chrono_now_iso() -> String {
    // tiny ISO-8601 UTC stamp without pulling in chrono for the skeleton
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    format!("{secs}")
}

fn broadcast<R: Runtime>(app: &tauri::AppHandle<R>, state: &AppState) {
    let snapshot = state.timer.lock().unwrap().clone();
    let _ = app.emit("timer-state", &snapshot);
    update_tray::<R>(app, &snapshot);
}

fn update_tray<R: Runtime>(app: &tauri::AppHandle<R>, t: &TimerState) {
    if let Some(item) = app.try_state::<TrayItems>() {
        let label = if t.running {
            format!("■ Stop — {}", t.description.clone().unwrap_or_default())
        } else {
            format!("Today: {}:{}", t.today_seconds / 3600, (t.today_seconds % 3600) / 60)
        };
        let _ = item.status.set_text(&label);
    }
}

struct TrayItems {
    status: MenuItem<Wry>,
    show: MenuItem<Wry>,
    resume: MenuItem<Wry>,
    stop: MenuItem<Wry>,
    quit: MenuItem<Wry>,
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
        .manage(AppState {
            timer: Mutex::new(TimerState {
                running: false,
                description: None,
                started_at: None,
                today_seconds: 0,
            }),
        })
        .setup(|app| {
            // ---- tray menu (dynamic item text; see ideation plan §Tray) ----
            let status = MenuItem::with_id(app, "status", "Today: 0:00", true, None::<&str>)?;
            let show = MenuItem::with_id(app, "show", "Show ToggLinux", true, None::<&str>)?;
            let resume = MenuItem::with_id(app, "resume", "Resume last entry", true, None::<&str>)?;
            let stop = MenuItem::with_id(app, "stop", "Stop timer", false, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&status, &sep1, &show, &resume, &stop, &sep2, &quit])?;

            let _tray = TrayIconBuilder::with_id("main-tray")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .tooltip("ToggLinux — unofficial Toggl Track client")
                .build(app)?;

            app.manage(TrayItems { status, show, resume, stop, quit });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Daemon-like behavior: closing the window hides it; the app keeps
            // running in the tray. Real exit happens via tray -> Quit.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            has_api_token,
            set_api_token,
            start_timer,
            stop_timer
        ])
        .run(tauri::generate_context!())
        .expect("error while running ToggLinux");
}
