//! `GET /api/v0/v/{vault_id}/manifest` — head manifest, decoded.
//!
//! Gated by [`crate::access::can_access_vault`]: non-owners get 404.
//! Reads the manifest blob raw via `coord.blobs()` — the hub doesn't
//! open vaults as a shareholder. The manifest blob itself is signed
//! plaintext DAG-CBOR; the sensitive bits are the encrypted
//! `SecretShare` payloads + the dir-body / file blobs, which the
//! browser decrypts client-side.
//!
//! What's exposed:
//! - `name`, `height`, `previous` — chain metadata
//! - `shares` — list of `{pubkey_hex, secret_share_hex}` so the
//!   browser can find its own share by pubkey match and hand the
//!   `secret_share_hex` to zim-wasm's `decryptBlob` as a `Sealed`
//!   envelope.
//! - `root_hash` — hash of the (encrypted) root dir-body blob. The
//!   browser fetches it via `/blob/{hash}` and decrypts client-side.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use zim_core::blobs::BlobStore;
use zim_core::fs::Manifest;
use zim_core::vault::VaultId;
use zim_peer::VaultLog;

use crate::access::can_access_vault;
use crate::http::auth::RequireUser;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct ManifestView {
    pub name: String,
    pub height: u64,
    pub root_hash: String,
    pub previous_hash: Option<String>,
    pub shares: Vec<ShareView>,
}

#[derive(Debug, Serialize)]
pub struct ShareView {
    pub pubkey: String,
    pub secret_share: String,
}

pub async fn handler(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
    Path(vault_id): Path<VaultId>,
) -> Response {
    if !can_access_vault(&state, &user, vault_id).await {
        return (StatusCode::NOT_FOUND, "vault not found").into_response();
    }
    let coord = state.service.peer().coord();
    let head = match coord.log().head(vault_id, None).await {
        Ok(h) => h,
        Err(_) => return (StatusCode::NOT_FOUND, "vault not found").into_response(),
    };
    let manifest: Manifest = match coord.blobs().get_cbor(&head.link).await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("vault {vault_id} manifest decode: {e}");
            return (StatusCode::NOT_FOUND, "manifest not available").into_response();
        }
    };

    let shares: Vec<ShareView> = manifest
        .shares()
        .iter()
        .map(|(pk, share)| ShareView {
            pubkey: pk.to_hex(),
            secret_share: share.secret_share().to_hex(),
        })
        .collect();

    let previous_hash = if *manifest.previous() == zim_core::linked_data::Link::default() {
        None
    } else {
        Some(manifest.previous().hash().to_string())
    };

    (
        StatusCode::OK,
        Json(ManifestView {
            name: manifest.name().to_string(),
            height: manifest.height(),
            root_hash: manifest.root().hash().to_string(),
            previous_hash,
            shares,
        }),
    )
        .into_response()
}
