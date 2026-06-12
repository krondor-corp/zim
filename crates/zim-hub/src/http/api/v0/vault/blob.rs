//! `GET /api/v0/v/{vault_id}/blob/{hash}` — fetch a ciphertext blob.
//!
//! Gated by [`crate::access::can_access_vault`]: non-owners get 404.
//! Tiered the same way the daemon's `ContentStore::get_metadata_bytes`
//! is: blob store first, then the head manifest's metadata pack —
//! dir bodies don't exist as standalone blobs, they ride inline in
//! the manifest. File content blobs are pinned and land in the blob
//! store during relay pull.

use std::str::FromStr;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use zim_core::blobs::BlobStore;
use zim_core::fs::Manifest;
use zim_core::iroh::Hash;
use zim_core::vault::VaultId;
use zim_peer::VaultLog;

use crate::access::can_access_vault;
use crate::http::auth::RequireUser;
use crate::state::AppState;

pub async fn handler(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
    Path((vault_id, hash_hex)): Path<(VaultId, String)>,
) -> Response {
    let hash = match Hash::from_str(&hash_hex) {
        Ok(h) => h,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid hash").into_response(),
    };
    if !can_access_vault(&state, &user, vault_id).await {
        return (StatusCode::NOT_FOUND, "vault not found").into_response();
    }
    let coord = state.service.peer().coord();
    let blobs = coord.blobs();
    if let Ok(bytes) = blobs.get(&hash).await {
        return ok_blob(bytes);
    }

    // Fall through to the head manifest's metadata pack — where dir
    // bodies live. The pack is keyed by the ciphertext hash, same
    // addressing as the blob store.
    if let Ok(head) = coord.log().head(vault_id, None).await {
        if let Ok(manifest) = blobs.get_cbor::<Manifest, _>(&head.link).await {
            if let Some(bytes) = manifest.metadata().get(&hash) {
                return ok_blob(Bytes::from(bytes.clone()));
            }
        }
    }

    tracing::debug!("blob {hash_hex} not available in store or metadata pack");
    (StatusCode::NOT_FOUND, "blob not available").into_response()
}

fn ok_blob(bytes: Bytes) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        bytes,
    )
        .into_response()
}
