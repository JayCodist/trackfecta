use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The part of `GET /me` that we use.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct UserInfo {
    pub id: i64,
    #[serde(default)]
    pub fullname: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    pub default_workspace_id: i64,
}

/// One item of `GET /workspaces`: an organisation the user belongs to.
/// In Toggl terms a workspace is an organisation. The app can switch
/// between them. The UI gets this type directly from the cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct Workspace {
    pub id: i64,
    pub name: String,
}

/// The part of a time entry that we use (`GET /me/time_entries`, and the
/// `POST`/`PUT` responses). A running entry has `stop: null`.
///
/// Every field except `id` is tolerant (`#[serde(default)]`). The delta feed
/// returns server-side deletions as partial records: `{id, at, deleted:true}`
/// with no other fields. Strict `start` and `workspace_id` fields used to make
/// the whole `Vec<TimeEntry>` fail to parse. That silently dropped every
/// change in that response, including the deletions.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct TimeEntry {
    pub id: i64,
    #[serde(default)]
    pub workspace_id: i64,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "epoch")]
    pub start: DateTime<Utc>,
    #[serde(default)]
    pub stop: Option<DateTime<Utc>>,
    /// Seconds. Negative or zero while running, server-side until the first
    /// sync.
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub project_id: Option<i64>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub billable: Option<bool>,
    /// v9 returns the last-modified time as `at`.
    #[serde(default, alias = "at")]
    pub updated_at: Option<DateTime<Utc>>,
    /// Delta sync deletion record: the entry was deleted server-side.
    #[serde(default)]
    pub deleted: Option<bool>,
    /// The real deletion marker that v9 sends. Entries deleted on the web or
    /// by another device come back in `GET /me/time_entries` (delta and full)
    /// as complete rows with `server_deleted_at` set. This is NOT the partial
    /// `{deleted:true}` shape. That field never appears in practice. A
    /// response row with this set must be marked deleted, never upserted as
    /// live. Ignoring it resurrected web-deleted entries in the cache.
    #[serde(default)]
    pub server_deleted_at: Option<DateTime<Utc>>,
}

impl TimeEntry {
    /// True when the server reports this entry as deleted, in either shape.
    pub fn is_deleted(&self) -> bool {
        self.deleted.unwrap_or(false) || self.server_deleted_at.is_some()
    }
}

/// The `serde(default)` target for `start`. Deletion records carry no
/// timestamp, and their value is never persisted. The sync loop sends deleted
/// rows to [`crate::store::Store::apply_tombstone`] instead of upsert.
fn epoch() -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_else(Utc::now)
}

/// An item from `GET /workspaces/{id}/projects`: the workspace project list
/// (name, Toggl color, and owning client).
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct WorkspaceProject {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub client_id: Option<i64>,
    #[serde(default)]
    pub workspace_id: Option<i64>,
    #[serde(default)]
    pub billable: Option<bool>,
    /// The color Toggl assigned to the project (hex, no `#` prefix). Used for
    /// the dot.
    #[serde(default)]
    pub color: Option<String>,
}

/// `GET /workspaces/{id}/clients` item.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Client {
    pub id: i64,
    pub name: String,
}

/// `GET /workspaces/{id}/tags` item.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct WorkspaceTag {
    pub id: i64,
    pub name: String,
}
