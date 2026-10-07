//! OS keyring access for the Toggl API token, with a graceful fallback.
//!
//! The token normally lives in the freedesktop Secret Service (gnome-keyring,
//! KWallet, ...). That is the right place: it persists across reboots and is
//! isolated from other apps. But on a strict snap the Secret Service is behind
//! the `password-manager-service` plug, which snapd treats as *manual-connect*
//! — so a fresh install cannot reach it until the user runs
//! `sudo snap connect`, and on a desktop with no Secret Service provider it is
//! never reachable at all.
//!
//! Rather than dead-end those users, `set_token` falls back to a file in the
//! app's own data directory when the keyring write fails. The fallback is
//! weaker at rest (a plain 0600 file in the user's own session) but it keeps
//! the "install and it just works" promise. When the keyring *is* available
//! we always prefer it and clear any stale fallback file, so connecting the
//! plug later transparently upgrades the user back to the keyring.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use keyring::Entry;

const SERVICE: &str = "io.github.jaycodist.trackfecta";
const USER: &str = "toggl_api_token";
/// Fallback file name, written under the app data dir when the keyring is
/// unreachable. Leading dot keeps it out of casual listings.
const FALLBACK_FILE: &str = ".toggl_api_token";

fn entry() -> Result<Entry, keyring::Error> {
    Entry::new(SERVICE, USER)
}

fn fallback_path(data_dir: &Path) -> PathBuf {
    data_dir.join(FALLBACK_FILE)
}

/// Read the token: keyring first, then the fallback file.
///
/// If the keyring holds it but a stale fallback file also exists (e.g. the
/// user connected the plug after saving once), the keyring wins and the file
/// is removed so the secret stops living in two places.
pub fn get_token(data_dir: &Path) -> Option<String> {
    if let Ok(token) = entry().ok()?.get_password() {
        let _ = fs::remove_file(fallback_path(data_dir));
        return Some(token);
    }
    read_fallback(data_dir)
}

/// Save the token to the keyring, falling back to the app data dir.
///
/// Only returns `Err` when *both* stores fail; the message then explains the
/// keyring problem (with the `snap connect` hint when that is the cause).
pub fn set_token(data_dir: &Path, token: &str) -> Result<(), String> {
    match entry().and_then(|e| e.set_password(token)) {
        Ok(()) => {
            let _ = fs::remove_file(fallback_path(data_dir));
            Ok(())
        }
        Err(keyring_err) => match write_fallback(data_dir, token) {
            Ok(()) => Ok(()),
            Err(file_err) => Err(format!(
                "{} The private-data fallback also failed: {}",
                friendly_keyring_error(&keyring_err),
                file_err
            )),
        },
    }
}

fn read_fallback(data_dir: &Path) -> Option<String> {
    fs::read_to_string(fallback_path(data_dir))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn write_fallback(data_dir: &Path, token: &str) -> io::Result<()> {
    fs::create_dir_all(data_dir)?;
    let path = fallback_path(data_dir);
    fs::write(&path, token)?;
    restrict_permissions(&path);
    Ok(())
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

/// Turn a raw keyring error into something actionable for the user.
///
/// Two failures are common and look similar:
///   * The snap's `password-manager-service` plug is not connected
///     (it is a manual-connect interface). Denied → show the fix command.
///   * No Secret Service provider exists at all (minimal window
///     managers without gnome-keyring/KWallet). Nothing to connect to.
///
/// Everything else passes through verbatim.
fn friendly_keyring_error(e: &keyring::Error) -> String {
    let msg = e.to_string();
    // A disconnected `password-manager-service` plug surfaces as an AppArmor
    // D-Bus denial (not PermissionDenied, which is the keyutils backend).
    if msg.contains("PermissionDenied") || msg.contains("AppArmor policy") {
        return format!(
            "The system keyring is not reachable from this sandbox. \
             If you installed Trackfecta as a snap, run \n\n  \
             sudo snap connect trackfecta:password-manager-service\n\n\
             once, then click Connect again to store the token securely. ({})",
            msg
        );
    }
    if msg.contains("NoService")
        || msg.contains("Service not found")
        || msg.contains("ServiceUnknown")
    {
        return format!(
            "No secret-service provider was found on this system. \
             Trackfecta stores its token in the freedesktop Secret Service \
             (gnome-keyring, KWallet, or equivalent). Enable one in your \
             desktop session and try again. ({})",
            msg
        );
    }
    msg
}

/// Remove the token from both the keyring and the fallback file.
pub fn delete_token(data_dir: &Path) {
    if let Ok(e) = entry() {
        let _ = e.delete_credential();
    }
    let _ = fs::remove_file(fallback_path(data_dir));
}
