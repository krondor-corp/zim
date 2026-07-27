use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_core::linked_data::Link;
use zim_did::Did;
use zim_peer::Effect;

use crate::daemon::api::client::ApiRequest;
use crate::daemon::api::v0::vault::extractor::VaultHandle;
use crate::daemon::state::vault_lookup_response;
use crate::ServiceState;
use zim_peer::VaultLookupError;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncRequest {
    #[serde(skip)]
    pub vault_id: zim_core::vault::VaultId,
    /// DID URL of the peer to pull from.
    pub peer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    pub peer: String,
    pub link: Link,
    pub height: u64,
}

pub async fn handler(
    State(state): State<ServiceState>,
    VaultHandle { vault, .. }: VaultHandle,
    Json(req): Json<SyncRequest>,
) -> Result<impl IntoResponse, SyncError> {
    let identity = Did::parse(&req.peer).map_err(|e| SyncError::BadPeer(e.to_string()))?;
    let peer = zim_did::resolve_pubkey(&identity, state.resolver().as_ref())
        .await
        .map_err(|e| SyncError::BadPeer(e.to_string()))?;
    state
        .peer()
        .coord()
        .execute(Effect::PullFromPeer {
            vault_id: vault.id(),
            peer_id: peer,
        })
        .await
        .map_err(|e| SyncError::Pull(e.to_string()))?;
    let head = vault
        .head()
        .await
        .map_err(|e| SyncError::Pull(e.to_string()))?;
    Ok((
        http::StatusCode::OK,
        Json(SyncResponse {
            peer: req.peer,
            link: head.link,
            height: head.height,
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Vault(#[from] VaultLookupError),
    #[error("invalid peer DID: {0}")]
    BadPeer(String),
    #[error("pull: {0}")]
    Pull(String),
}

impl IntoResponse for SyncError {
    fn into_response(self) -> axum::response::Response {
        match self {
            SyncError::Vault(e) => vault_lookup_response(e),
            SyncError::BadPeer(_) => {
                (http::StatusCode::BAD_REQUEST, self.to_string()).into_response()
            }
            SyncError::Pull(_) => {
                (http::StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
            }
        }
    }
}

impl ApiRequest for SyncRequest {
    type Response = SyncResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let url = base_url
            .join(&format!("/api/v0/vaults/{}/sync", self.vault_id))
            .unwrap();
        client.post(url).json(&self)
    }
}
