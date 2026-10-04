//! Idle detection. Reports how long the user has been away from the
//! keyboard, with no API requests.
//!
//! Backends, tried in order:
//!   1. GNOME: `org.gnome.Mutter.IdleMonitor.GetIdletime`. Works on both
//!      X11 and Wayland, because it is a GNOME Shell service.
//!   2. KDE: `org.freedesktop.ScreenSaver.GetIdleTime`.
//!   3. Otherwise: no backend. The feature stays off without an error.
//!
//! Both backends are local D-Bus reads. They cost no API budget, so the
//! poll can be frequent (10 seconds while a timer runs).
//!
//! Why a plain OS thread: `zbus::blocking` runs its own mini runtime on
//! every call. Calling it from the Tauri async runtime nests one runtime
//! inside another (the "async sandwich") and panics. A dedicated thread
//! keeps the blocking calls off that runtime.
//!
//! Why the idle window is exact even with a coarse poll: the service
//! returns milliseconds since the last user input. So the last-activity
//! time is `now - idle_ms`. The poll interval only changes when we notice
//! the idle, not the start or end we record.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Wry};
use zbus::proxy;
use zbus::proxy::CacheProperties;

use crate::state::PendingIdle;

/// True once the monitor thread has started. Guards against a second spawn.
static STARTED: AtomicBool = AtomicBool::new(false);

/// GNOME Shell idle service.
#[proxy(
    interface = "org.gnome.Mutter.IdleMonitor",
    default_service = "org.gnome.Mutter.IdleMonitor",
    default_path = "/org/gnome/Mutter/IdleMonitor/Core",
    gen_async = false
)]
trait MutterIdle {
    /// Milliseconds since the last user input.
    fn get_idletime(&self) -> zbus::Result<u64>;
}

/// freedesktop ScreenSaver service (KDE).
#[proxy(
    interface = "org.freedesktop.ScreenSaver",
    default_service = "org.freedesktop.ScreenSaver",
    default_path = "/ScreenSaver",
    gen_async = false
)]
trait KdeScreenSaver {
    /// Milliseconds since the last user input.
    #[zbus(no_autostart)]
    fn get_idle_time(&self) -> zbus::Result<u32>;
}

/// Starts the monitor thread once. Safe to call from setup.
pub fn spawn(app: AppHandle<Wry>) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    if std::thread::Builder::new()
        .name("idle-monitor".into())
        .spawn(move || run(app))
        .is_err()
    {
        STARTED.store(false, Ordering::SeqCst);
    }
}

/// Reads the idle gate: is the feature on, the threshold in seconds, and a
/// timer running. The thread polls this every cycle, so a Settings change
/// takes effect within one poll.
fn read_gate(app: &AppHandle<Wry>) -> (bool, i64, bool) {
    let st = app.state::<crate::state::AppState>();
    let enabled = st
        .store
        .as_ref()
        .and_then(|s| s.get_setting("idle_enabled"))
        .map(|v| v != "false")
        .unwrap_or(true);
    let minutes = st
        .store
        .as_ref()
        .and_then(|s| s.get_setting("idle_threshold_min"))
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|m| (1..=240).contains(m))
        .unwrap_or(5);
    let running = st.timer.lock().unwrap().running;
    (enabled, minutes * 60, running)
}

/// Publishes the detected backend name for the Settings screen. Broadcasts
/// on change, so the note updates even on a quiet session.
fn set_backend(app: &AppHandle<Wry>, name: &str) {
    let changed = {
        let st = app.state::<crate::state::AppState>();
        let mut idle = st.idle.lock().unwrap();
        if idle.backend != name {
            idle.backend = name.to_string();
            true
        } else {
            false
        }
    };
    if changed {
        crate::logger::log("info", &format!("idle: backend = {name}"));
        crate::state::broadcast(app);
    }
}

/// Reads idle milliseconds from the first backend that answers. Returns the
/// backend name with the value.
fn read_idle(
    mutter: &Option<MutterIdleProxy>,
    kde: &Option<KdeScreenSaverProxy>,
) -> Option<(&'static str, i64)> {
    if let Some(m) = mutter {
        if let Ok(ms) = m.get_idletime() {
            return Some(("gnome", ms as i64));
        }
    }
    if let Some(k) = kde {
        if let Ok(ms) = k.get_idle_time() {
            return Some(("kde", ms as i64));
        }
    }
    None
}

fn run(app: AppHandle<Wry>) {
    let Ok(conn) = zbus::blocking::Connection::session() else {
        set_backend(&app, "none");
        return;
    };
    // CacheProperties::No stops the builder from reading properties at
    // build time. The method call is the real availability test.
    let mutter = MutterIdleProxy::builder(&conn)
        .cache_properties(CacheProperties::No)
        .build()
        .ok();
    let kde = KdeScreenSaverProxy::builder(&conn)
        .cache_properties(CacheProperties::No)
        .build()
        .ok();
    if mutter.is_none() && kde.is_none() {
        set_backend(&app, "none");
        return;
    }

    // The start of the idle period we are watching, once it crosses the
    // threshold. Cleared when the user returns or the timer stops.
    let mut armed: Option<i64> = None;

    loop {
        let (enabled, threshold_s, running) = read_gate(&app);
        let now = chrono::Utc::now().timestamp();

        match read_idle(&mutter, &kde) {
            Some((backend, idle_ms)) => {
                set_backend(&app, backend);
                // Last user input, to the second. Exact, not tied to the
                // poll interval.
                let last_activity = now - idle_ms.div_euclid(1000);

                if !enabled || !running {
                    armed = None;
                } else if idle_ms >= threshold_s * 1000 {
                    // Idle. Remember when it began, once.
                    if armed.is_none() {
                        armed = Some(last_activity);
                    }
                } else if let Some(start) = armed.take() {
                    // The user is back. Report the idle gap it just ended.
                    let end = last_activity;
                    if end > start {
                        emit_dialog(&app, start, end);
                    }
                }
            }
            None => {
                // The service went away mid-session. Stop watching.
                set_backend(&app, "none");
                armed = None;
            }
        }

        // Poll fast while a timer runs, slow otherwise. Both are local
        // reads, so cost is only a wakeup.
        std::thread::sleep(Duration::from_secs(if running { 10 } else { 60 }));
    }
}

/// Records a pending idle prompt and tells the UI to show it. The window is
/// raised through the main thread, because a hidden tray session would
/// otherwise never show the prompt.
fn emit_dialog(app: &AppHandle<Wry>, start: i64, end: i64) {
    let pending = {
        let st = app.state::<crate::state::AppState>();
        let description = st.timer.lock().unwrap().description.clone();
        let p = PendingIdle {
            idle_start: start,
            idle_end: end,
            idle_seconds: end - start,
            running_description: description,
        };
        st.idle.lock().unwrap().pending = Some(p.clone());
        p
    };
    let _ = app.emit("idle-dialog", &pending);

    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = handle.get_webview_window("main") {
            let _ = w.show();
            let _ = w.set_focus();
        }
    });
}
