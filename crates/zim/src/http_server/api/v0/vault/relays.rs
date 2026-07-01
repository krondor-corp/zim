use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};

use crate::http_server::api::client::ApiRequest;
use crate::http_server::api::v0::vault::extractor::VaultHandle;
use crate::service_state::vault_lookup_response;
use zim_peer::VaultLookupError;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelaysRequest {
    #[serde(skip)]
    pub vault_id: zim_core::vault::VaultId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelaysResponse {
    pub relays: Vec<RelayInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayInfo {
    /// DID of the ephemeral recipient peer.
    pub recipient: String,
    /// DID of the always-on via peer.
    pub via: String,
}

pub async fn handler(
    VaultHandle { vault, .. }: VaultHandle,
    Json(_req): Json<RelaysRequest>,
) -> Result<impl IntoResponse, RelaysError> {
    let relays = vault
        .list_relays()
        .iter()
        .map(|(recipient, via)| RelayInfo {
            recipient: recipient.to_did().to_string(),
            via: via.to_did().to_string(),
        })
        .collect();
    Ok((http::StatusCode::OK, Json(RelaysResponse { relays })).into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum RelaysError {
    #[error(transparent)]
    Vault(#[from] VaultLookupError),
}

impl IntoResponse for RelaysError {
    fn into_response(self) -> axum::response::Response {
        match self {
            RelaysError::Vault(e) => vault_lookup_response(e),
        }
    }
}

impl ApiRequest for RelaysRequest {
    type Response = RelaysResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let url = base_url
            .join(&format!("/api/v0/vault/{}/relays", self.vault_id))
            .unwrap();
        client.post(url).json(&self)
    }
}
