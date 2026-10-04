//! HTTP layer for Toggl Track API v9.
//! Auth: HTTP Basic. The API token is the username, and the literal
//! `api_token` is the password.

use reqwest::{Client, Method, Response};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use thiserror::Error;

use super::models::{
    Client as ClientInfo, TimeEntry, UserInfo, WorkspaceProject, WorkspaceTag,
};
use crate::budget::Budget;

const BASE: &str = "https://api.track.toggl.com/api/v9";

#[derive(Debug, Error)]
pub enum TogglError {
    #[error("network error: {0}")]
    Network(String),
    #[error("token rejected (unauthorized)")]
    Unauthorized,
    #[error("rate limited, retry in {retry_after}s")]
    RateLimited { retry_after: u64 },
    #[error("API error {status}: {body}")]
    Api { status: u16, body: String },
}

pub struct TogglClient {
    http: Client,
    token: String,
    budget: Arc<Budget>,
}

impl TogglClient {
    pub fn new(token: &str, budget: Arc<Budget>) -> Self {
        let http = Client::builder()
            .user_agent(concat!("ToggLinux/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client");
        Self {
            http,
            token: token.to_string(),
            budget,
        }
    }

    fn request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        // Single choke point. Every outbound API call is accounted against
        // the rolling 1-hour budget. The free plan allows about 30 requests
        // per hour.
        self.budget.record();
        self.http
            .request(method, format!("{BASE}{path}"))
            .basic_auth(&self.token, Some("api_token"))
    }

    async fn decode<T: DeserializeOwned>(resp: Response) -> Result<T, TogglError> {
        let status = resp.status();
        if status.as_u16() == 401 {
            return Err(TogglError::Unauthorized);
        }
        if status.as_u16() == 429 {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(5);
            return Err(TogglError::RateLimited { retry_after });
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            crate::logger::log("error", &format!("API {status}: {body}"));
            return Err(TogglError::Api {
                status: status.as_u16(),
                body,
            });
        }
        let text = resp
            .text()
            .await
            .map_err(|e| TogglError::Network(format!("read failed: {e}")))?;
        serde_json::from_str::<T>(&text).map_err(|e| {
            // A parse failure used to be silent. Keep the raw body, truncated,
            // so a schema mismatch (for example a missing `start`) can be
            // diagnosed.
            crate::logger::log(
                "error",
                &format!(
                    "bad response: {e}; body={}",
                    &text.chars().take(400).collect::<String>()
                ),
            );
            TogglError::Network(format!("bad response: {e}"))
        })
    }

    /// `GET /me`. Validates the token and finds the default workspace.
    pub async fn me(&self) -> Result<UserInfo, TogglError> {
        let resp = self
            .request(Method::GET, "/me")
            .send()
            .await
            .map_err(|e| TogglError::Network(e.to_string()))?;
        Self::decode(resp).await
    }

    /// Time entries for a workspace (`GET /me/time_entries`), v9 delta form.
    /// `since` (Unix seconds) returns only the entries modified at or after
    /// that instant. Server-side deletions are included, as full rows with
    /// `server_deleted_at` set. See
    /// [`crate::toggl::TimeEntry::is_deleted`]. Repeated polls then transfer
    /// (and cost) as little as possible.
    /// `None` returns about 10 days.
    pub async fn me_time_entries(
        &self,
        workspace_id: i64,
        since: Option<i64>,
    ) -> Result<Vec<TimeEntry>, TogglError> {
        let mut q = vec![("workspace_id", workspace_id.to_string())];
        if let Some(s) = since {
            q.push(("since", s.to_string()));
        }
        let resp = self
            .request(Method::GET, "/me/time_entries")
            .query(&q)
            .send()
            .await
            .map_err(|e| TogglError::Network(e.to_string()))?;
        Self::decode(resp).await
    }

    /// `GET /workspaces/{id}/projects`: the full project list for a workspace
    /// (name, color, and client_id). One request per session, cached in
    /// SQLite.
    pub async fn workspace_projects(
        &self,
        workspace_id: i64,
    ) -> Result<Vec<WorkspaceProject>, TogglError> {
        let resp = self
            .request(Method::GET, &format!("/workspaces/{workspace_id}/projects"))
            .send()
            .await
            .map_err(|e| TogglError::Network(e.to_string()))?;
        Self::decode(resp).await
    }

    /// `GET /workspaces/{id}/clients`: the client list for a workspace.
    pub async fn workspace_clients(
        &self,
        workspace_id: i64,
    ) -> Result<Vec<ClientInfo>, TogglError> {
        let resp = self
            .request(Method::GET, &format!("/workspaces/{workspace_id}/clients"))
            .send()
            .await
            .map_err(|e| TogglError::Network(e.to_string()))?;
        Self::decode(resp).await
    }

    /// `GET /workspaces/{id}/tags`: the tag list for a workspace. This feeds
    /// the tag picker. `/me/interests` was removed upstream and returns 404.
    pub async fn workspace_tags(
        &self,
        workspace_id: i64,
    ) -> Result<Vec<WorkspaceTag>, TogglError> {
        let resp = self
            .request(Method::GET, &format!("/workspaces/{workspace_id}/tags"))
            .send()
            .await
            .map_err(|e| TogglError::Network(e.to_string()))?;
        Self::decode(resp).await
    }

    /// Creates a manual (non-running) entry
    /// (`POST /workspaces/{id}/time_entries`).
    /// v9 needs a flat time-entry body. Do not use the v8 wrapper
    /// `{"time_entry": {...}}`.
    pub async fn create_entry(&self, body: &serde_json::Value) -> Result<TimeEntry, TogglError> {
        let wid = body["workspace_id"]
            .as_i64()
            .ok_or_else(|| TogglError::Api {
                status: 0,
                body: "create_entry: missing workspace_id".into(),
            })?;
        let resp = self
            .request(Method::POST, &format!("/workspaces/{wid}/time_entries"))
            .json(body)
            .send()
            .await
            .map_err(|e| TogglError::Network(e.to_string()))?;
        Self::decode(resp).await
    }

    /// Updates an existing entry (`PUT /workspaces/{wid}/time_entries/{id}`).
    /// v9 takes a flat time-entry body, the same shape as create, with no
    /// wrapper.
    pub async fn update_entry(
        &self,
        workspace_id: i64,
        entry_id: i64,
        body: &serde_json::Value,
    ) -> Result<TimeEntry, TogglError> {
        let resp = self
            .request(
                Method::PUT,
                &format!("/workspaces/{workspace_id}/time_entries/{entry_id}"),
            )
            .json(body)
            .send()
            .await
            .map_err(|e| TogglError::Network(e.to_string()))?;
        Self::decode(resp).await
    }

    /// Deletes an entry (`DELETE /workspaces/{wid}/time_entries/{id}`).
    /// 200/204 is OK. A 404 also counts as success. The entry is already gone
    /// from the server, for example deleted in the web app first. Then the
    /// local deletion mark is correct.
    pub async fn delete_entry(
        &self,
        workspace_id: i64,
        entry_id: i64,
    ) -> Result<(), TogglError> {
        let resp = self
            .request(
                Method::DELETE,
                &format!("/workspaces/{workspace_id}/time_entries/{entry_id}"),
            )
            .send()
            .await
            .map_err(|e| TogglError::Network(e.to_string()))?;
        let status = resp.status();
        if status.as_u16() == 404 {
            return Ok(());
        }
        if status.as_u16() == 401 {
            return Err(TogglError::Unauthorized);
        }
        if status.as_u16() == 429 {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(5);
            return Err(TogglError::RateLimited { retry_after });
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(TogglError::Api {
                status: status.as_u16(),
                body,
            });
        }
        Ok(())
    }
}
