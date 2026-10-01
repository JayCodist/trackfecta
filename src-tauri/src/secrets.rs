//! OS keyring access for the Toggl API token.

use keyring::Entry;

const SERVICE: &str = "com.togglinux.app";
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
        .map_err(|e| e.to_string())
}
