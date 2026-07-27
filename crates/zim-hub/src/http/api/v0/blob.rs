//! `/api/v0/blob` — stateless content-addressed ciphertext store.
//!
//! The hub runs a single global blob store shared across vaults. Blobs
//! are content-addressed (blake3) so the hash *is* the identity — there's
//! no per-vault namespace to enforce, and ciphertext reveals nothing
//! without the vault secret. Access is gated only on "is the caller an
//! enrolled user" via [`RequireUser`]; the encryption is the real access
//! boundary. This is what the browser's `HubBlobStore` speaks: it has a
//! hash and a session JWT, never a vault id.
//!
//! - `PUT /api/v0/blob` — store raw bytes, return the blake3 hash. The
//!   blob is ephemeral until the manifest referencing it is committed via
//!   `write_head`, which tags it persistent.
//! - `GET /api/v0/blob/{hash}` — fetch ciphertext by hash.
//!
//! Dir bodies are *not* served here — they ride inline in the manifest's
//! metadata pack, so the browser (`WasmFs`) reads them from the decrypted
//! manifest it already holds, never as standalone blobs.

use std::str::FromStr;

use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{body::Bytes, Json, Router};
use zim_core::blobs::BlobStore;
use zim_core::linked_data::Hash;
// Shared wire type — mirrored by `zim_api::hub::vault::PutBlobRequest`.
use zim_api::hub::blob::WriteBlobResponse;

use crate::http::auth::RequireUser;
use crate::state::AppState;

/// Per-blob upload cap. Blobs are buffered in memory before hashing, so this
/// bounds memory per request. Generous for everyday files; truly large files
/// would want chunked/streaming upload (a future change).
const MAX_BLOB_BYTES: usize = 256 * 1024 * 1024;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", put(write))
        .route("/:hash", get(read))
        // Raise axum's default 2 MiB body limit for blob writes.
        .layer(DefaultBodyLimit::max(MAX_BLOB_BYTES))
        .with_state(state)
}

async fn write(
    State(state): State<AppState>,
    RequireUser(_user): RequireUser,
    body: Bytes,
) -> Response {
    let blobs = state.peer.coord().blobs();
    let len = body.len();
    match blobs.put(body.to_vec()).await {
        Ok(hash) => {
            tracing::info!(hash = %hex::encode(hash.as_bytes()), bytes = len, "blob PUT");
            (
                StatusCode::OK,
                Json(WriteBlobResponse {
                    hash: hex::encode(hash.as_bytes()),
                }),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("blob put failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "blob store error").into_response()
        }
    }
}

async fn read(
    State(state): State<AppState>,
    RequireUser(_user): RequireUser,
    Path(hash_hex): Path<String>,
) -> Response {
    let hash = match Hash::from_str(&hash_hex) {
        Ok(h) => h,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid hash").into_response(),
    };
    let blobs = state.peer.coord().blobs();
    match blobs.get(&hash).await {
        Ok(bytes) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/octet-stream"),
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "blob not available").into_response(),
    }
}
