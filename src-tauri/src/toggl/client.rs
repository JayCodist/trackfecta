//! HTTP layer for Toggl Track API v9.
//! Auth: HTTP Basic — the API token is the *username*, literal `api_token` the password.

use chrono::{DateTime, Utc};
use reqwest::{Client, Method, Response};
use serde::de::DeserializeOwned;
use thiserror::Error;

use super::models::{TimeEntry, UserInfo};

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
}

impl TogglClient {
    pub fn new(token: &str) -> Self {
        let http = Client::builder()
            .user_agent(concat!("ToggLinux/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client");
        Self {
            http,
            token: token.to_string(),
        }
    }

    fn request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
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
            return Err(TogglError::Api {
                status: status.as_u16(),
                body,
            });
        }
        resp.json::<T>()
            .await
            .map_err(|e| TogglError::Network(format!("bad response: {e}")))
    }

    /// `GET /me` — validates the token and discovers the default workspace.
    pub async fn me(&self) -> Result<UserInfo, TogglError> {
        let resp = self
            .request(Method::GET, "/me")
            .send()
            .await
            .map_err(|e| TogglError::Network(e.to_string()))?;
        Self::decode(resp).await
    }

    /// Time entries between two instants (`GET /me/time_entries`).
    pub async fn time_entries(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<TimeEntry>, TogglError> {
        let resp = self
            .request(Method::GET, "/me/time_entries")
            .query(&[
                (
                    "start_date",
                    &from.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                ),
                (
                    "end_date",
                    &to.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                ),
            ])
            .send()
            .await
            .map_err(|e| TogglError::Network(e.to_string()))?;
        Self::decode(resp).await
    }

    /// Start a running entry (`POST /workspaces/{id}/time_entries/start`).
    pub async fn start_entry(
        &self,
        workspace_id: i64,
        description: Option<&str>,
    ) -> Result<TimeEntry, TogglError> {
        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let body = serde_json::json!({
            "workspace_id": workspace_id,
            "start": now,
            "stop": null,
            "created_with": "ToggLinux",
            "description": description,
        });
        let resp = self
            .request(
                Method::POST,
                &format!("/workspaces/{workspace_id}/time_entries/start"),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| TogglError::Network(e.to_string()))?;
        Self::decode(resp).await
    }

    /// Stop a running entry (`PUT /workspaces/{id}/time_entries/{id}/stop`).
    pub async fn stop_entry(
        &self,
        workspace_id: i64,
        entry_id: i64,
    ) -> Result<TimeEntry, TogglError> {
        let resp = self
            .request(
                Method::PUT,
                &format!("/workspaces/{workspace_id}/time_entries/{entry_id}/stop"),
            )
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| TogglError::Network(e.to_string()))?;
        Self::decode(resp).await
    }
}
