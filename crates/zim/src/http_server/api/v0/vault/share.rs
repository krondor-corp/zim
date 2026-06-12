use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_core::linked_data::Link;
use zim_did::Identity;
use zim_peer::Effect;

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
    // Resolve to a concrete iroh pubkey. `Identity::Key` is a no-op;
    // `Identity::Web` triggers an HTTPS GET against the hub's
    // `did.json` and picks the first peer-purpose verification
    // method.
    let peer = zim_did::resolve_pubkey(&identity, state.peer().resolver().as_ref())
        .await
        .map_err(|e| ShareError::BadPeer(e.to_string()))?;
    // `vault.add_share` takes the concrete pubkey directly — DID
    // resolution stops at this boundary, the storage layer is
    // pubkey-shaped.
    vault
        .add_share(peer)
        .map_err(|e| ShareError::Share(e.to_string()))?;
    let link = vault
        .save()
        .await
        .map_err(|e| ShareError::Save(e.to_string()))?;
    let head = vault
        .head()
        .await
        .map_err(|e| ShareError::Save(e.to_string()))?;

    // Tell the new sharee about the vault so it appears on their side.
    // Fire-and-forget — the share response returns as soon as alice's
    // local state is committed; the announce flows over iroh in the
    // background. If the peer is offline they'll miss this offer;
    // re-running `share` re-fires it (intentional, see
    // docs/research/optimistic-share-acceptance.md).
    //
    // We deliberately do NOT also fire `announce_head` to the new
    // sharee — they're being bootstrapped via OfferShare, and a
    // concurrent AnnounceHead → PullFromPeer races with the
    // bootstrap's `apply_chain` writes (UNIQUE constraint on the
    // vault log). The peer learns the head as part of OfferShare;
    // future saves on this peer will get normal AnnounceHead pushes
    // once they're fully bootstrapped.
    if let Err(e) = state
        .peer()
        .coord()
        .submit(Effect::OfferShare {
            peer_id: peer,
            vault_id: vault.id(),
            head: head.clone(),
        })
        .await
    {
        tracing::warn!(
            peer = req.peer,
            vault_id = %vault.id(),
            "failed to enqueue OfferShare: {e}"
        );
    }
    // Announce to every *pre-existing* shareholder (the new sharee
    // is being handled by OfferShare above; sending them an extra
    // AnnounceHead races with their bootstrap).
    state
        .peer()
        .announce_head_except(&vault, head.clone(), Some(&peer))
        .await;

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
