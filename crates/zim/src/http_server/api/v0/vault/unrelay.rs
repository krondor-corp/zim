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
pub struct UnrelayRequest {
    #[serde(skip)]
    pub vault_id: zim_core::vault::VaultId,
    /// DID URL of the recipient whose relay entry to remove.
    pub recipient: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnrelayResponse {
    pub recipient: String,
    pub link: Link,
    pub height: u64,
}

pub async fn handler(
    State(state): State<ServiceState>,
    VaultHandle { mut vault, .. }: VaultHandle,
    Json(req): Json<UnrelayRequest>,
) -> Result<impl IntoResponse, UnrelayError> {
    let identity =
        Identity::parse(&req.recipient).map_err(|e| UnrelayError::BadPeer(e.to_string()))?;
    let recipient_pk = zim_did::resolve_pubkey(&identity, state.resolver().as_ref())
        .await
        .map_err(|e| UnrelayError::BadPeer(e.to_string()))?;
    vault
        .remove_relay(recipient_pk)
        .map_err(|e| UnrelayError::Unrelay(e.to_string()))?;
    let link = vault
        .save()
        .await
        .map_err(|e| UnrelayError::Save(e.to_string()))?;
    let head = vault
        .head()
        .await
        .map_err(|e| UnrelayError::Save(e.to_string()))?;
    state.peer().announce_head(&vault, head.clone()).await;
    Ok((
        http::StatusCode::OK,
        Json(UnrelayResponse {
            recipient: req.recipient,
            link,
            height: head.height,
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum UnrelayError {
    #[error(transparent)]
    Vault(#[from] VaultLookupError),
    #[error("invalid peer DID: {0}")]
    BadPeer(String),
    #[error("unrelay: {0}")]
    Unrelay(String),
    #[error("save: {0}")]
    Save(String),
}

impl IntoResponse for UnrelayError {
    fn into_response(self) -> axum::response::Response {
        match self {
            UnrelayError::Vault(e) => vault_lookup_response(e),
            UnrelayError::BadPeer(_) => {
                (http::StatusCode::BAD_REQUEST, self.to_string()).into_response()
            }
            _ => (http::StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response(),
        }
    }
}

impl ApiRequest for UnrelayRequest {
    type Response = UnrelayResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let url = base_url
            .join(&format!("/api/v0/vault/{}/unrelay", self.vault_id))
            .unwrap();
        client.post(url).json(&self)
    }
}
