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
pub struct ShareRequest {
    #[serde(skip)]
    pub vault_id: zim_core::vault::VaultId,
    /// DID URL of the peer being granted access.
    /// Examples: `did:key:z6Mk…`, `did:web:hub.example.com:u:alice`.
    pub peer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareResponse {
    pub peer: String,
    pub link: Link,
    pub height: u64,
}

pub async fn handler(
    State(state): State<ServiceState>,
    VaultHandle { mut vault, .. }: VaultHandle,
    Json(req): Json<ShareRequest>,
) -> Result<impl IntoResponse, ShareError> {
    let identity = Identity::parse(&req.peer).map_err(|e| ShareError::BadPeer(e.to_string()))?;
    // *Share to a DID* — one operation across every shape. `Identity::Key`
    // yields a single direct reach; `Identity::Web` resolves the document
    // and yields one reach per verification method (the whole device set),
    // each sealed to its own client and routed via the host. The storage
    // layer stays pubkey-shaped — resolution stops at this boundary.
    let reaches = zim_did::resolve_reaches(&identity, state.resolver().as_ref())
        .await
        .map_err(|e| ShareError::BadPeer(e.to_string()))?;
    for reach in &reaches {
        vault
            .add_reach(reach.clone())
            .map_err(|e| ShareError::Share(e.to_string()))?;
    }
    let link = vault
        .save()
        .await
        .map_err(|e| ShareError::Save(e.to_string()))?;
    let head = vault
        .head()
        .await
        .map_err(|e| ShareError::Save(e.to_string()))?;

    // Announce the new head to every shareholder. Each turns it into a
    // pull: a freshly-added sharee bootstraps the vault (it accepts
    // because we're in its address book — sharing is mutual by
    // construction), an existing one fast-forwards. Fire-and-forget; if
    // a peer is offline it misses this and re-running `share` re-fires.
    state.peer().announce_head(&vault, head.clone()).await;

    Ok((
        http::StatusCode::OK,
        Json(ShareResponse {
            peer: req.peer,
            link,
            height: head.height,
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum ShareError {
    #[error(transparent)]
    Vault(#[from] VaultLookupError),
    #[error("invalid peer hex: {0}")]
    BadPeer(String),
    #[error("share: {0}")]
    Share(String),
    #[error("save: {0}")]
    Save(String),
}

impl IntoResponse for ShareError {
    fn into_response(self) -> axum::response::Response {
        match self {
            ShareError::Vault(e) => vault_lookup_response(e),
            ShareError::BadPeer(_) => {
                (http::StatusCode::BAD_REQUEST, self.to_string()).into_response()
            }
            _ => (http::StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response(),
        }
    }
}

impl ApiRequest for ShareRequest {
    type Response = ShareResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let url = base_url
            .join(&format!("/api/v0/vault/{}/share", self.vault_id))
            .unwrap();
        client.post(url).json(&self)
    }
}
