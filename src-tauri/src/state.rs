//! Shared application state: the timer snapshot, the Toggl session, the tray
//! handles, and the broadcast helper. The broadcast helper sends the state to
//! the webview and updates the tray.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::menu::MenuItem;
use tauri::{AppHandle, Emitter, Manager, Wry};
use tokio::sync::Notify;

use crate::budget::Budget;
use crate::store::{EntryRow, Store};
use crate::toggl::{TogglClient, UserInfo};

#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnStatus {
    /// A token is in the keyring. The app is checking it against the API.
    Verifying,
    /// No token, or the token was rejected or expired.
    Disconnected,
    /// Authenticated with Toggl.
    Connected,
}

/// The state snapshot the app sends to the UI on every change. The event
/// name is timer-state.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TimerState {
    pub running: bool,
    pub description: Option<String>,
    /// Unix seconds, as a string. The UI counts the elapsed time itself.
    pub started_at: Option<String>,
    /// Seconds tracked today, not including the running entry. The UI adds
    /// the elapsed time of the running entry.
    pub today_seconds: i64,
    pub status: ConnStatus,
    /// Recent entries from the local cache, newest first. Reading the cache
    /// uses no API requests.
    pub entries: Vec<EntryRow>,
    /// Requests left in the current rolling hour. The free plan allows about
    /// 30 per hour.
    pub requests_left: usize,
    /// True while a server quota block (402 or 429) is active. All sync is
    /// paused until `next_sync_in` elapses.
    pub blocked: bool,
    /// Seconds until the next request can go out: the longer of our window
    /// refill and the server block. Zero when nothing is waiting.
    pub next_sync_in: i64,
    /// The idle-detection backend in use: gnome, kde, or none. The Settings
    /// screen shows it. See idle.rs.
    pub idle_backend: String,
}

impl Default for TimerState {
    fn default() -> Self {
        Self {
            running: false,
            description: None,
            started_at: None,
            today_seconds: 0,
            status: ConnStatus::Verifying,
            entries: Vec::new(),
            requests_left: 0,
            blocked: false,
            next_sync_in: 0,
            idle_backend: String::new(),
        }
    }
}

/// An idle period that just ended while a timer ran. The UI asks the user
/// to keep, discard, or split it. Unix seconds.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingIdle {
    pub idle_start: i64,
    pub idle_end: i64,
    pub idle_seconds: i64,
    /// The running entry description, for the dialog text.
    pub running_description: Option<String>,
}

/// Idle-monitor state shared between the monitor thread and the commands.
pub struct IdleState {
    /// The prompt waiting for the user's answer. The thread sets it, a
    /// command clears it when the user answers.
    pub pending: Option<PendingIdle>,
    /// The backend the monitor thread detected: gnome, kde, or none.
    pub backend: String,
}

pub struct Session {
    pub client: Arc<TogglClient>,
    pub user: UserInfo,
}

/// The data of the most recent entry. The "Resume" item in the tray menu
/// reuses it.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastEntry {
    pub description: Option<String>,
    pub project_id: Option<i64>,
    pub tags: Vec<String>,
    pub billable: bool,
}

pub struct AppState {
    pub timer: Mutex<TimerState>,
    pub session: Mutex<Option<Session>>,
    /// The API token, cached in memory as a copy of the keyring value. The
    /// poll loop uses it to retry a connection after a temporary network
    /// failure. The user does not have to enter it again.
    pub token: Mutex<Option<String>>,
    /// The server id of the running entry. None when stopped or not
    /// connected.
    pub entry_id: Mutex<Option<i64>>,
    /// The data of the most recent entry. The "Resume" item in the tray menu
    /// reuses it.
    pub last_entry: Mutex<Option<LastEntry>>,
    /// Wakes the sync loop early: on window focus, start/stop, or re-auth.
    pub wakeup: Arc<Notify>,
    /// The SQLite cache. None if the cache failed to open. Then the UI runs
    /// on live data only.
    pub store: Option<Arc<Store>>,
    /// The rolling 1-hour API request budget. The free plan allows about 30
    /// requests per hour.
    pub budget: Arc<Budget>,
    /// True while a connect is in progress. The poll loop must not start a
    /// second one. Without this guard, a focus wakeup during the startup
    /// connect made every connect-time request fire twice.
    pub connecting: Arc<std::sync::atomic::AtomicBool>,
    /// Idle-monitor state. See idle.rs.
    pub idle: Mutex<IdleState>,
}

impl AppState {
    pub fn new(store: Option<Arc<Store>>) -> Self {
        Self {
            timer: Mutex::new(TimerState::default()),
            session: Mutex::new(None),
            token: Mutex::new(None),
            entry_id: Mutex::new(None),
            last_entry: Mutex::new(None),
            wakeup: Arc::new(Notify::new()),
            store,
            budget: Arc::new(Budget::new()),
            connecting: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            idle: Mutex::new(IdleState {
                pending: None,
                backend: "none".into(),
            }),
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

/// Sends the current timer snapshot to the UI and updates the tray text.
pub fn broadcast(app: &AppHandle) {
    let snapshot = {
        let st = app.state::<AppState>();
        {
            let mut t = st.timer.lock().unwrap();
            // The entries and the today total come from the cache. This is
            // instant and uses no API requests. (sync::refresh sets a
            // fallback total from live data when there is no cache.)
            // Do the recompute on the stored state. The tray ticker reads
            // today_seconds from it. An update of the snapshot only left the
            // panel icon showing just the running elapsed time.
            if let Some(store) = st.store.as_ref() {
                t.entries = store.entries_since(start_of_window(window_days(st.store.as_ref())));
                let today = start_of_today();
                t.today_seconds = t
                    .entries
                    .iter()
                    .filter(|r| r.stop.is_some() && r.start >= today)
                    .map(|r| r.duration.max(0.0) as i64)
                    .sum();
            }
            t.requests_left = st.budget.remaining();
            t.blocked = !st.budget.until_blocked().is_zero();
            t.next_sync_in = st.budget.until_refill().as_secs() as i64;
            t.idle_backend = st.idle.lock().unwrap().backend.clone();
            t.clone()
        }
    };
    let _ = app.emit("timer-state", &snapshot);
    update_tray(app, &snapshot);
    // The panel icon shows the live day total (see tray.rs). Draw it again on
    // every state change. Start, stop, and edits show up at once.
    crate::tray::refresh(app);
}

/// Local midnight today, as Unix seconds. Toggl counts "today" as the
/// user's own day.
pub fn start_of_today() -> i64 {
    let now = chrono::Local::now();
    let midnight = now.date_naive().and_time(chrono::NaiveTime::MIN);
    midnight
        .and_local_timezone(chrono::Local)
        .single()
        .or_else(|| midnight.and_local_timezone(chrono::Local).latest())
        .map(|t| t.timestamp())
        .unwrap_or_else(|| now.timestamp())
}

/// Default history window for the list view, in days (one month).
pub const DEFAULT_WINDOW_DAYS: i64 = 31;

/// The list window in days. This is the window_days setting, which the
/// "Load earlier entries" button grows. The value is limited to a sane
/// range.
pub fn window_days(store: Option<&Arc<Store>>) -> i64 {
    store
        .and_then(|s| s.get_setting("window_days"))
        .and_then(|v| v.parse().ok())
        .filter(|d: &i64| (7..=366).contains(d))
        .unwrap_or(DEFAULT_WINDOW_DAYS)
}

/// Start of the entry-list window: local midnight, `days - 1` back.
pub fn start_of_window(days: i64) -> i64 {
    let now = chrono::Local::now();
    let day = now.date_naive() - chrono::Duration::days(days - 1);
    let midnight = day.and_time(chrono::NaiveTime::MIN);
    midnight
        .and_local_timezone(chrono::Local)
        .single()
        .or_else(|| midnight.and_local_timezone(chrono::Local).latest())
        .map(|t| t.timestamp())
        .unwrap_or_else(|| now.timestamp() - (days - 1) * 86_400)
}

fn update_tray(app: &AppHandle, t: &TimerState) {
    let Some(items) = app.try_state::<TrayItems>() else {
        return;
    };
    let label = if t.running {
        format!(
            "■ Stop: {}",
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
        .last_entry
        .lock()
        .unwrap()
        .clone();
    let _ = items.resume.set_enabled(!t.running && last.is_some());
    if let Some(last) = last {
        let label = last.description.unwrap_or_else(|| "Untitled".into());
        let _ = items.resume.set_text(format!("Resume: {label}"));
    }
}

/// Drops the session after a bad or expired token, and tells the UI to show
/// the Auth screen.
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
        // An unanswered idle prompt is meaningless without a session.
        st.idle.lock().unwrap().pending = None;
    }
    broadcast(app);
}

/// Shows a non-fatal error banner in the UI.
pub fn emit_toast(app: &AppHandle, message: &str) {
    let _ = app.emit("toast", message.to_string());
}
