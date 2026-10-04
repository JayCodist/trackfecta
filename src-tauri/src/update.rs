//! App self-update over GitHub Releases.
//!
//! Two paths, both driven by the Tauri updater plugin:
//!   * A background check once per day. It reads the release manifest
//!     (`latest.json`) that the release workflow publishes. It does not use
//!     the Toggl request budget: GitHub is a different host.
//!   * A manual check from the Settings screen.
//!
//! When an update is found, the app emits `update-available`. The Settings
//! About section shows a banner. The actions depend on how the app runs:
//!   * An AppImage can replace its own binary, so it offers Install, which
//!     downloads, swaps the file, and relaunches.
//!   * A `.deb` or `.rpm` install needs a package manager and root, so those
//!     builds only offer the release page.
//!   * A Flatpak or Snap build is updated by its store, so it shows a note
//!     and the release page. The background check does not run there.
//!
//! The update manifest is signed. The plugin verifies the signature against
//! the public key in `tauri.conf.json` before it installs anything.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::UpdaterExt;

/// The home page for the release list. The per-version URL is built from the
/// tag pattern the release workflow uses (`v<version>`).
const RELEASES_BASE: &str = "https://github.com/JayCodist/trackfecta/releases";

/// What the UI needs to know about the newest release. Serialized to the webview.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    /// True when a newer signed release exists.
    pub available: bool,
    /// The version the app is running now.
    pub current_version: String,
    /// The newest version, when one is available.
    pub latest_version: Option<String>,
    /// Release notes from the manifest, when present.
    pub notes: Option<String>,
    /// The release page for the newest version.
    pub release_url: String,
    /// True when a package store owns updates (Flatpak or Snap). The UI then
    /// hides the in-app install button and points at the store instead.
    pub store_managed: bool,
    /// True when the in-app install is reliable: the AppImage build. deb and
    /// rpm show the release page instead of installing in app.
    pub can_install: bool,
    /// True when the check reached the server and got a valid answer. False
    /// when offline or the manifest is missing. The UI shows "up to date"
    /// only when this is true and no update is available.
    pub checked: bool,
}

/// True when running inside a Flatpak or Snap sandbox. Those stores deliver
/// updates, so the in-app self-update path does not apply.
fn is_store_managed() -> bool {
    std::env::var("FLATPAK_ID").is_ok() || std::env::var("SNAP").is_ok()
}

/// True when running as an AppImage. The AppImage runtime sets APPIMAGE to
/// the bundle path. This is the one build that can replace its own binary.
fn is_appimage() -> bool {
    std::env::var("APPIMAGE").is_ok()
}

/// Builds a result for the current version. `checked` says whether the
/// server answered. Used when there is no update, or the check cannot run.
fn current_only(current_version: String, checked: bool) -> Info {
    Info {
        available: false,
        current_version,
        latest_version: None,
        notes: None,
        release_url: RELEASES_BASE.to_string(),
        store_managed: is_store_managed(),
        can_install: false,
        checked,
    }
}

/// The last check result, kept in `AppState`. `get_update_info` returns it so
/// the UI can show the banner after a reload or a fetch-on-mount.
pub struct UpdateState {
    pub last: Mutex<Option<Info>>,
}

impl UpdateState {
    pub fn new() -> Self {
        Self {
            last: Mutex::new(None),
        }
    }
}

fn release_url_for(version: &str) -> String {
    if version.starts_with('v') {
        format!("{RELEASES_BASE}/tag/{version}")
    } else {
        format!("{RELEASES_BASE}/tag/v{version}")
    }
}

/// Runs one check against the release manifest. Returns the result and, when
/// an update is available, emits `update-available` and stores the info.
///
/// A failure (offline, or no published release yet) is not an error to the
/// user. It logs and returns the current version marked as up to date, so the
/// Settings row can say "You are up to date" or "Check failed" without noise.
async fn run_check(app: &AppHandle<Wry>) -> Info {
    let current = app.package_info().version.to_string();
    let updater = match app.updater() {
        Ok(u) => u,
        Err(e) => {
            crate::logger::log("warn", &format!("updater init: {e}"));
            return current_only(current, false);
        }
    };
    match updater.check().await {
        Ok(Some(update)) => {
            let info = Info {
                available: true,
                current_version: update.current_version.clone(),
                latest_version: Some(update.version.clone()),
                notes: update.body.clone(),
                release_url: release_url_for(&update.version),
                store_managed: is_store_managed(),
                can_install: is_appimage(),
                checked: true,
            };
            crate::logger::log(
                "info",
                &format!(
                    "update available: {}",
                    info.latest_version.as_deref().unwrap_or("?")
                ),
            );
            if let Some(state) = app.try_state::<UpdateState>() {
                *state.last.lock().unwrap() = Some(info.clone());
            }
            let _ = app.emit("update-available", &info);
            info
        }
        Ok(None) => {
            crate::logger::log("info", "update check: up to date");
            let info = current_only(current, true);
            if let Some(state) = app.try_state::<UpdateState>() {
                *state.last.lock().unwrap() = Some(info.clone());
            }
            info
        }
        Err(e) => {
            // Offline, or no release published yet. Stay quiet in the UI.
            crate::logger::log("warn", &format!("update check failed: {e}"));
            current_only(current, false)
        }
    }
}

/// The daily background check. It waits after startup so it does not compete
/// with the first Toggl connect, then checks once per day.
///
/// Under Flatpak or Snap it does not run: those stores deliver updates, and a
/// self-update check would only add a background request the store handles.
pub fn spawn(app: AppHandle<Wry>) {
    if is_store_managed() {
        crate::logger::log("info", "update check disabled under a package store");
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            run_check(&app).await;
            tokio::time::sleep(Duration::from_secs(24 * 3600)).await;
        }
    });
}

/// Checks for an update now, from the Settings screen. Returns the result for
/// the caller to render.
pub async fn check_now(app: &AppHandle<Wry>) -> Info {
    run_check(app).await
}

/// Downloads and installs the newest update, then relaunches. Re-checks first
/// so it installs the current manifest rather than a stored handle.
pub async fn install(app: &AppHandle<Wry>) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no update is available".to_string())?;
    crate::logger::log("info", "update: download start");
    let mut downloaded = 0usize;
    update
        .download_and_install(
            |chunk, _total| {
                downloaded += chunk;
                crate::logger::log("info", &format!("update: downloaded {downloaded} bytes"));
            },
            || {
                crate::logger::log("info", "update: download finished, installing");
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    crate::logger::log("info", "update: installed, restarting");
    // cleanup_before_exit already runs through the updater's on_before_exit
    // hook. Restart picks up the new binary (AppImage) or the new package.
    app.restart();
}

/// Opens the release page in the default browser. The fallback when an in-app
/// install is not possible, such as a deb or rpm without a polkit agent.
pub fn open_release(app: &AppHandle<Wry>) -> Result<(), String> {
    let url = app
        .try_state::<UpdateState>()
        .and_then(|s| s.last.lock().unwrap().clone())
        .map(|i| i.release_url)
        .unwrap_or_else(|| RELEASES_BASE.to_string());
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| e.to_string())
}
