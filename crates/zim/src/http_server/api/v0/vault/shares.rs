use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};

use crate::http_server::api::client::ApiRequest;
use crate::http_server::api::v0::vault::extractor::VaultHandle;
use crate::service_state::vault_lookup_response;
use crate::ServiceState;
use zim_peer::VaultLookupError;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SharesRequest {
    #[serde(skip)]
    pub vault_id: zim_core::vault::VaultId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharesResponse {
    /// DID URL of the daemon's own identity — lets the CLI mark
    /// which row corresponds to "you" without a separate identity
    /// round-trip. Format: `did:key:z…` (daemons are always
    /// `Identity::Key`).
    pub you: String,
    pub shares: Vec<ShareInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareInfo {
    /// DID URL of the share recipient.
    pub peer: String,
}

pub async fn handler(
    State(state): State<ServiceState>,
    VaultHandle { vault, .. }: VaultHandle,
    Json(_req): Json<SharesRequest>,
) -> Result<impl IntoResponse, SharesError> {
    let shares = vault
        .manifest()
        .shares()
        .iter()
        .map(|(_, share)| ShareInfo {
            peer: share.identity().to_string(),
        })
        .collect();
    let you = zim_did::Identity::Key(state.peer().secret().public()).to_string();
    Ok((http::StatusCode::OK, Json(SharesResponse { you, shares })).into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum SharesError {
    #[error(transparent)]
    Vault(#[from] VaultLookupError),
}

impl IntoResponse for SharesError {
    fn into_response(self) -> axum::response::Response {
        match self {
            SharesError::Vault(e) => vault_lookup_response(e),
        }
    }
}

impl ApiRequest for SharesRequest {
    type Response = SharesResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let url = base_url
            .join(&format!("/api/v0/vault/{}/shares", self.vault_id))
            .unwrap();
        client.post(url).json(&self)
    }
}
