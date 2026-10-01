//! Shared application state: timer snapshot, Toggl session, tray handles,
//! and the broadcast helper that pushes state to the webview + tray.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::menu::MenuItem;
use tauri::{AppHandle, Emitter, Manager, Wry};
use tokio::sync::Notify;

use crate::toggl::{TogglClient, UserInfo};

#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnStatus {
    /// Token found in keyring, validating against the API.
    Verifying,
    /// No token / token rejected / expired.
    Disconnected,
    /// Authenticated with Toggl.
    Connected,
}

/// Snapshot pushed to the UI on every change (`timer-state` event).
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TimerState {
    pub running: bool,
    pub description: Option<String>,
    /// Unix seconds (as string — the UI does its own ticking).
    pub started_at: Option<String>,
    /// Seconds tracked today, EXCLUDING the running entry (UI adds elapsed).
    pub today_seconds: i64,
    pub status: ConnStatus,
}

impl Default for TimerState {
    fn default() -> Self {
        Self {
            running: false,
            description: None,
            started_at: None,
            today_seconds: 0,
            status: ConnStatus::Verifying,
        }
    }
}

pub struct Session {
    pub client: Arc<TogglClient>,
    pub user: UserInfo,
}

pub struct AppState {
    pub timer: Mutex<TimerState>,
    pub session: Mutex<Option<Session>>,
    /// Cached API token (mirror of the keyring) so the poll loop can retry a
    /// connection after a transient network failure without re-prompting.
    pub token: Mutex<Option<String>>,
    /// Server id of the running entry (None when stopped/not connected).
    pub entry_id: Mutex<Option<i64>>,
    /// Description of the most recent entry — powers tray "Resume".
    pub last_description: Mutex<Option<String>>,
    /// Wakes the sync loop early (focus, start/stop, re-auth).
    pub wakeup: Arc<Notify>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            timer: Mutex::new(TimerState::default()),
            session: Mutex::new(None),
            token: Mutex::new(None),
            entry_id: Mutex::new(None),
            last_description: Mutex::new(None),
            wakeup: Arc::new(Notify::new()),
        }
    }
}

pub struct TrayItems {
    pub status: MenuItem<Wry>,
    #[allow(dead_code)]
    pub show: MenuItem<Wry>,
    pub resume: MenuItem<Wry>,
    pub stop: MenuItem<Wry>,
    #[allow(dead_code)]
    pub quit: MenuItem<Wry>,
}

/// Emit the current timer snapshot to the UI and refresh the tray text.
pub fn broadcast(app: &AppHandle) {
    let snapshot = {
        let st = app.state::<AppState>();
        let snapshot = st.timer.lock().unwrap().clone();
        snapshot
    };
    let _ = app.emit("timer-state", &snapshot);
    update_tray(app, &snapshot);
}

fn update_tray(app: &AppHandle, t: &TimerState) {
    let Some(items) = app.try_state::<TrayItems>() else {
        return;
    };
    let label = if t.running {
        format!(
            "■ Stop — {}",
            t.description.clone().unwrap_or_else(|| "Untitled".into())
        )
    } else {
        format!(
            "Today: {}:{:02}",
            t.today_seconds / 3600,
            (t.today_seconds % 3600) / 60
        )
    };
    let _ = items.status.set_text(&label);
    let _ = items.stop.set_enabled(t.running);

    let last = app
        .state::<AppState>()
        .last_description
        .lock()
        .unwrap()
        .clone();
    let _ = items.resume.set_enabled(!t.running && last.is_some());
    if let Some(desc) = last {
        let _ = items.resume.set_text(format!("Resume: {desc}"));
    }
}

/// Drop the session (bad/expired token) and tell the UI to show Auth.
pub fn mark_disconnected(app: &AppHandle) {
    {
        let st = app.state::<AppState>();
        let mut t = st.timer.lock().unwrap();
        t.status = ConnStatus::Disconnected;
        t.running = false;
        t.description = None;
        t.started_at = None;
        *st.session.lock().unwrap() = None;
        *st.entry_id.lock().unwrap() = None;
    }
    broadcast(app);
}

/// Non-fatal error banner in the UI.
pub fn emit_toast(app: &AppHandle, message: &str) {
    let _ = app.emit("toast", message.to_string());
}
