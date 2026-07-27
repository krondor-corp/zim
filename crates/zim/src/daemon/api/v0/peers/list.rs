use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};

use crate::daemon::api::client::ApiRequest;
use crate::ServiceState;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse {
    pub peers: Vec<PeerInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub nick: String,
    /// DID URL of the peer.
    pub did: String,
    /// Whether this contact is trusted (auto-shared into owned vaults).
    #[serde(default)]
    pub trusted: bool,
    pub added_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

pub async fn handler(
    State(state): State<ServiceState>,
    Json(_req): Json<ListRequest>,
) -> Result<impl IntoResponse, ListError> {
    use zim_peer::PeerStore;
    let entries = state
        .peers()
        .list()
        .await
        .map_err(|e| ListError::Load(e.to_string()))?;
    let peers = entries
        .into_iter()
        .map(|e| PeerInfo {
            nick: e.nick,
            did: e.identity.to_string(),
            trusted: e.trusted,
            added_at: e.added_at,
            notes: e.notes,
        })
        .collect();
    Ok((http::StatusCode::OK, Json(ListResponse { peers })).into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum ListError {
    #[error("load contacts: {0}")]
    Load(String),
}

impl IntoResponse for ListError {
    fn into_response(self) -> axum::response::Response {
        (http::StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}

impl ApiRequest for ListRequest {
    type Response = ListResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        client
            .post(base_url.join("/api/v0/peers/list").unwrap())
            .json(&self)
    }
}
