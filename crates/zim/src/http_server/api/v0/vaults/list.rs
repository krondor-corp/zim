use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_core::vault::VaultId;

use crate::http_server::api::client::ApiRequest;
use crate::ServiceState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse {
    pub vaults: Vec<VaultInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultInfo {
    pub vault_id: VaultId,
    /// `None` if the daemon couldn't open this vault (see `error`).
    #[serde(default)]
    pub name: Option<String>,
    /// Why the vault failed to open, if it did. Surface this in the
    /// CLI rather than silently rendering a blank name.
    #[serde(default)]
    pub error: Option<String>,
}

pub async fn handler(
    State(state): State<ServiceState>,
    Json(_req): Json<ListRequest>,
) -> Result<impl IntoResponse, ListError> {
    let vaults = state
        .peer()
        .list_vaults()
        .await
        .map_err(|e| ListError::List(e.to_string()))?
        .into_iter()
        .map(|l| VaultInfo {
            vault_id: l.id,
            name: l.name,
            error: l.error,
        })
        .collect();
    Ok((http::StatusCode::OK, Json(ListResponse { vaults })).into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum ListError {
    #[error("list vaults: {0}")]
    List(String),
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
            .post(base_url.join("/api/v0/vaults/list").unwrap())
            .json(&self)
    }
}
