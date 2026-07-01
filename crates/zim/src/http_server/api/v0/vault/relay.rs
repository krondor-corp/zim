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
pub struct RelayRequest {
    #[serde(skip)]
    pub vault_id: zim_core::vault::VaultId,
    /// DID URL of the ephemeral recipient peer (e.g. browser session key).
    pub recipient: String,
    /// DID URL of the always-on via peer (e.g. the hub).
    pub via: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayResponse {
    pub recipient: String,
    pub via: String,
    pub link: Link,
    pub height: u64,
}

pub async fn handler(
    State(state): State<ServiceState>,
    VaultHandle { mut vault, .. }: VaultHandle,
    Json(req): Json<RelayRequest>,
) -> Result<impl IntoResponse, RelayError> {
    let recipient_identity =
        Identity::parse(&req.recipient).map_err(|e| RelayError::BadPeer(e.to_string()))?;
    let via_identity = Identity::parse(&req.via).map_err(|e| RelayError::BadPeer(e.to_string()))?;
    let recipient_pk = zim_did::resolve_pubkey(&recipient_identity, state.resolver().as_ref())
        .await
        .map_err(|e| RelayError::BadPeer(e.to_string()))?;
    let via_pk = zim_did::resolve_pubkey(&via_identity, state.resolver().as_ref())
        .await
        .map_err(|e| RelayError::BadPeer(e.to_string()))?;
    vault
        .add_share_via(recipient_pk, Some(Identity::Key(via_pk)))
        .map_err(|e| RelayError::Save(e.to_string()))?;
    let link = vault
        .save()
        .await
        .map_err(|e| RelayError::Save(e.to_string()))?;
    let head = vault
        .head()
        .await
        .map_err(|e| RelayError::Save(e.to_string()))?;
    state.peer().announce_head(&vault, head.clone()).await;
    Ok((
        http::StatusCode::OK,
        Json(RelayResponse {
            recipient: req.recipient,
            via: req.via,
            link,
            height: head.height,
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error(transparent)]
    Vault(#[from] VaultLookupError),
    #[error("invalid peer DID: {0}")]
    BadPeer(String),
    #[error("save: {0}")]
    Save(String),
}

impl IntoResponse for RelayError {
    fn into_response(self) -> axum::response::Response {
        match self {
            RelayError::Vault(e) => vault_lookup_response(e),
            RelayError::BadPeer(_) => {
                (http::StatusCode::BAD_REQUEST, self.to_string()).into_response()
            }
            _ => (http::StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response(),
        }
    }
}

impl ApiRequest for RelayRequest {
    type Response = RelayResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let url = base_url
            .join(&format!("/api/v0/vault/{}/relay", self.vault_id))
            .unwrap();
        client.post(url).json(&self)
    }
}
