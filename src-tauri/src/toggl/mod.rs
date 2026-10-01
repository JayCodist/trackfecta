//! Minimal Toggl Track API v9 client.
//! Base: https://api.track.toggl.com/api/v9 — HTTP Basic: token as username, literal `api_token` as password.

mod client;
mod models;

pub use client::{TogglClient, TogglError};
pub use models::UserInfo;
