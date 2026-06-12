use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_core::linked_data::Link;
use zim_did::Identity;

use crate::http_server::api::client::ApiRequest;
use crate::http_server::api::v0::vault::extractor::VaultHandle;
use crate::service_state::vault_lookup_response;
use crate::ServiceState;
use zim_peer::VaultLookupError;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UnshareRequest {
    #[serde(skip)]
    pub vault_id: zim_core::vault::VaultId,
    /// DID URL of the peer being un-shared.
    pub peer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnshareResponse {
    pub peer: String,
    pub link: Link,
    pub height: u64,
}

pub async fn handler(
    State(state): State<ServiceState>,
    VaultHandle { mut vault, .. }: VaultHandle,
    Json(req): Json<UnshareRequest>,
) -> Result<impl IntoResponse, UnshareError> {
    let identity = Identity::parse(&req.peer).map_err(|e| UnshareError::BadPeer(e.to_string()))?;
    let peer = zim_did::resolve_pubkey(&identity, state.peer().resolver().as_ref())
        .await
        .map_err(|e| UnshareError::BadPeer(e.to_string()))?;
    vault
        .remove_share(peer)
        .map_err(|e| UnshareError::Unshare(e.to_string()))?;
    let link = vault
        .save()
        .await
        .map_err(|e| UnshareError::Save(e.to_string()))?;
    let head = vault
        .head()
        .await
        .map_err(|e| UnshareError::Save(e.to_string()))?;
    state.peer().announce_head(&vault, head.clone()).await;
    Ok((
        http::StatusCode::OK,
        Json(UnshareResponse {
            peer: req.peer,
            link,
            height: head.height,
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum UnshareError {
    #[error(transparent)]
    Vault(#[from] VaultLookupError),
    #[error("invalid peer DID: {0}")]
    BadPeer(String),
    #[error("unshare: {0}")]
    Unshare(String),
    #[error("save: {0}")]
    Save(String),
}

impl IntoResponse for UnshareError {
    fn into_response(self) -> axum::response::Response {
        match self {
            UnshareError::Vault(e) => vault_lookup_response(e),
            UnshareError::BadPeer(_) => {
                (http::StatusCode::BAD_REQUEST, self.to_string()).into_response()
            }
            _ => (http::StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response(),
        }
    }
}

impl ApiRequest for UnshareRequest {
    type Response = UnshareResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let url = base_url
            .join(&format!("/api/v0/vault/{}/unshare", self.vault_id))
            .unwrap();
        client.post(url).json(&self)
    }
}
