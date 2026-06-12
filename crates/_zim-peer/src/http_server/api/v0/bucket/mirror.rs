//! Relay peer authorization endpoints (T-016c).
//!
//! Relays are pre-authorized peers that can sync this bucket's published-set
//! blobs without holding the bucket secret. They live in `manifest.relays`
//! (a flat `Vec<PublicKey>`) and are private metadata for the owner's deploy —
//! only owners may add, remove, or list them. The wire-layer gating that
//! makes `manifest.relays` load-bearing is T-016b's `GatedBlobsHandler`.
//!
//! Endpoints (flat-route convention matching the rest of `/api/v0/bucket/*`):
//! - `POST /relays/list` — return the current relay set for a bucket.
//! - `POST /relays/add` — add a peer pubkey to `manifest.relays`.
//! - `POST /relays/remove` — remove a peer pubkey from `manifest.relays`.

use axum::extract::{Json, State};
use axum::response::{IntoResponse, Response};
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zim_core::fs::FsError;

use zim_crypto::PublicKey;

use crate::http_server::api::client::ApiRequest;
use crate::ServiceState;

// =============================================================
// list
// =============================================================

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct ListRelaysRequest {
    /// Bucket ID
    #[arg(long)]
    pub bucket_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRelaysResponse {
    pub bucket_id: Uuid,
    /// Hex-encoded pubkeys of all currently-authorized relays.
    pub relays: Vec<String>,
}

pub async fn list_handler(
    State(state): State<ServiceState>,
    Json(req): Json<ListRelaysRequest>,
) -> Result<impl IntoResponse, RelayError> {
    let mount = state.peer().mount(req.bucket_id).await?;
    let relays = mount
        .list_relays()
        .await
        .into_iter()
        .map(|r| r.identity().to_hex())
        .collect();
    Ok((
        http::StatusCode::OK,
        Json(ListRelaysResponse {
            bucket_id: req.bucket_id,
            relays,
        }),
    )
        .into_response())
}

// =============================================================
// add
// =============================================================

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct AddRelayRequest {
    /// Bucket ID
    #[arg(long)]
    pub bucket_id: Uuid,
    /// Hex-encoded peer pubkey (Ed25519, 64 hex chars)
    #[arg(long)]
    pub peer_public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddRelayResponse {
    pub bucket_id: Uuid,
    pub peer_public_key: String,
    pub new_bucket_link: String,
}

pub async fn add_handler(
    State(state): State<ServiceState>,
    Json(req): Json<AddRelayRequest>,
) -> Result<impl IntoResponse, RelayError> {
    let peer_pk = PublicKey::from_hex(&req.peer_public_key)
        .map_err(|e| RelayError::InvalidPublicKey(e.to_string()))?;

    let mut mount = state.peer().mount(req.bucket_id).await?;
    mount.add_relay(zim_core::fs::Relay::new(peer_pk)).await;
    let new_link = state.peer().save_mount(&mount).await?;

    tracing::info!(
        "Added relay {} to bucket {}, new link {}",
        req.peer_public_key,
        req.bucket_id,
        new_link.hash()
    );

    Ok((
        http::StatusCode::OK,
        Json(AddRelayResponse {
            bucket_id: req.bucket_id,
            peer_public_key: req.peer_public_key,
            new_bucket_link: new_link.hash().to_string(),
        }),
    )
        .into_response())
}

// =============================================================
// remove
// =============================================================

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct RemoveRelayRequest {
    /// Bucket ID
    #[arg(long)]
    pub bucket_id: Uuid,
    /// Hex-encoded peer pubkey to remove
    #[arg(long)]
    pub peer_public_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveRelayResponse {
    pub bucket_id: Uuid,
    pub peer_public_key: String,
    /// `true` if the pubkey was in the relay set and has been removed;
    /// `false` if it wasn't there (idempotent).
    pub removed: bool,
    pub new_bucket_link: String,
}

pub async fn remove_handler(
    State(state): State<ServiceState>,
    Json(req): Json<RemoveRelayRequest>,
) -> Result<impl IntoResponse, RelayError> {
    let peer_pk = PublicKey::from_hex(&req.peer_public_key)
        .map_err(|e| RelayError::InvalidPublicKey(e.to_string()))?;

    let mount = state.peer().mount(req.bucket_id).await?;
    let removed = mount.remove_relay(peer_pk).await?;
    let new_link = state.peer().save_mount(&mount).await?;

    tracing::info!(
        "Removed relay {} from bucket {} (was_present={}), new link {}",
        req.peer_public_key,
        req.bucket_id,
        removed,
        new_link.hash()
    );

    Ok((
        http::StatusCode::OK,
        Json(RemoveRelayResponse {
            bucket_id: req.bucket_id,
            peer_public_key: req.peer_public_key,
            removed,
            new_bucket_link: new_link.hash().to_string(),
        }),
    )
        .into_response())
}

// =============================================================
// shared error type
// =============================================================

#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("Invalid public key: {0}")]
    InvalidPublicKey(String),
    #[error("Fs error: {0}")]
    Fs(#[from] FsError),
}

impl IntoResponse for RelayError {
    fn into_response(self) -> Response {
        match &self {
            RelayError::InvalidPublicKey(msg) => (
                http::StatusCode::BAD_REQUEST,
                format!("Invalid public key: {}", msg),
            )
                .into_response(),
            RelayError::Fs(_) => (
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "Unexpected error".to_string(),
            )
                .into_response(),
        }
    }
}

// =============================================================
// client builders for the CLI
// =============================================================

impl ApiRequest for ListRelaysRequest {
    type Response = ListRelaysResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let full_url = base_url.join("/api/v0/bucket/relays/list").unwrap();
        client.post(full_url).json(&self)
    }
}

impl ApiRequest for AddRelayRequest {
    type Response = AddRelayResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let full_url = base_url.join("/api/v0/bucket/relays/add").unwrap();
        client.post(full_url).json(&self)
    }
}

impl ApiRequest for RemoveRelayRequest {
    type Response = RemoveRelayResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let full_url = base_url.join("/api/v0/bucket/relays/remove").unwrap();
        client.post(full_url).json(&self)
    }
}
