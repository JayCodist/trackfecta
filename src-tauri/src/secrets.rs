//! OS keyring access for the Toggl API token.

use keyring::Entry;

const SERVICE: &str = "io.github.jaycodist.trackfecta";
const USER: &str = "toggl_api_token";
fn entry() -> Result<Entry, keyring::Error> {
    Entry::new(SERVICE, USER)
}

pub fn get_token() -> Option<String> {
    entry().ok()?.get_password().ok()
}

pub fn set_token(token: &str) -> Result<(), String> {
    entry()
        .map_err(|e| e.to_string())?
        .set_password(token)
        .map_err(|e| friendly_keyring_error(&e))
}

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
    if msg.contains("PermissionDenied") {
        return format!(
            "The system keyring is not reachable from this sandbox. \
             If you installed Trackfecta as a snap, run \n\n  \
             sudo snap connect trackfecta:password-manager-service\n\n\
             once, then click Connect again. ({})",
            msg
        );
    }
    if msg.contains("NoService") || msg.contains("Service not found") {
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

pub fn delete_token() {
    if let Ok(e) = entry() {
        let _ = e.delete_credential();
    }
}
