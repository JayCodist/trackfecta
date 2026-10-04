//! Minimal Toggl Track API v9 client.
//! Base: https://api.track.toggl.com/api/v9. HTTP Basic: the token as the
//! username, and the literal `api_token` as the password.

mod client;
mod models;

pub use client::{TogglClient, TogglError};
pub use models::{
    Client, TimeEntry, UserInfo, WorkspaceProject, WorkspaceTag,
};
