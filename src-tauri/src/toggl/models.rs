use chrono::{DateTime, Utc};
use serde::Deserialize;

/// Subset of `GET /me` that we care about.
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

/// Subset of a time entry (`GET /me/time_entries`, `POST/PUT` responses).
/// A running entry has `stop: null`.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct TimeEntry {
    pub id: i64,
    pub workspace_id: i64,
    #[serde(default)]
    pub description: Option<String>,
    pub start: DateTime<Utc>,
    #[serde(default)]
    pub stop: Option<DateTime<Utc>>,
    /// Seconds; negative/zero while running server-side until first sync.
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub project_id: Option<i64>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}
