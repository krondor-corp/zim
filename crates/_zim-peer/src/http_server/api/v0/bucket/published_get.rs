//! Gateway-facing read endpoint: `POST /api/v0/bucket/published/get`.
//!
//! Resolves a `display_path` in the bucket's `published_set` and returns
//! the corresponding ciphertext bytes (as the response body, mime
//! `application/octet-stream`) plus the envelope JSON in the
//! `X-Zim-Envelope` response header. The wasm client (`zim-wasm`) consumes
//! both to decrypt: `decryptBlob(envelopeJson, ciphertext)`.
//!
//! Public read — does **not** require the requester to be a bucket member.
//! Only paths actually present in `published_set` are served; everything
//! else is 404. The bucket secret is never exposed; only the per-entry
//! `Secret` (which decrypts exactly that one node) is shipped to the
//! client.
//!
//! Folder browsing is out of scope for this endpoint — it returns 415 if
//! the matching entry is a `Folder`. A separate `published/list` endpoint
//! will handle folder listings in a later slice.

use axum::extract::{Json, State};
use axum::http::header::HeaderValue;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zim_core::blobs::BlobStore;
use zim_core::blobs::BlobsStore;
use zim_core::fs::{AbsPath, Fs, FsError, Entry};
use zim_protocol::log::BucketLogProvider;

use crate::ServiceState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedGetRequest {
    pub bucket_id: Uuid,
    pub display_path: String,
}

pub async fn handler(
    State(state): State<ServiceState>,
    Json(req): Json<PublishedGetRequest>,
) -> Result<Response, PublishedGetError> {
    tracing::info!(
        "PUBLISHED GET API: bucket={} display_path={}",
        req.bucket_id,
        req.display_path
    );

    let (head_link, _height) = state
        .peer()
        .log_provider()
        .head(req.bucket_id, None)
        .await
        .map_err(|e| PublishedGetError::Fs(FsError::Backing(anyhow::anyhow!("head: {e}"))))?;

    let manifest = state.peer().blobs()
        .get_cbor::<zim_core::fs::Manifest, _>(&head_link)
        .await
        .map_err(|e| PublishedGetError::Fs(FsError::Blob(e)))?;

    let abs_path = if req.display_path.starts_with('/') {
        req.display_path.clone()
    } else {
        format!("/{}", req.display_path)
    };
    let lookup_path = AbsPath::new(&abs_path).ok_or(PublishedGetError::NotPublished)?;
    let leaf = manifest
        .published()
        .get(&lookup_path)
        .ok_or(PublishedGetError::NotPublished)?;

    if leaf.is_dir() {
        return Err(PublishedGetError::FolderListingNotSupported);
    }

    let Entry::File { link, secret, .. } = leaf else {
        return Err(PublishedGetError::FolderListingNotSupported);
    };

    let ciphertext = state.peer().blobs().get(&link.hash()).await?;

    let envelope_json = serde_json::to_string(&serde_json::json!({
        "kind": "public",
        "secret": hex::encode(secret.bytes()),
    }))
    .map_err(|e| PublishedGetError::Fs(FsError::Backing(anyhow::anyhow!("envelope encode: {e}"))))?;

    let envelope_header = HeaderValue::from_str(&envelope_json).map_err(|e| {
        PublishedGetError::Fs(FsError::Backing(anyhow::anyhow!("envelope header: {e}")))
    })?;

    let mut response = (
        http::StatusCode::OK,
        [("Content-Type", "application/octet-stream")],
        ciphertext,
    )
        .into_response();
    response
        .headers_mut()
        .insert("X-Zim-Envelope", envelope_header);
    Ok(response)
}

#[derive(Debug, thiserror::Error)]
pub enum PublishedGetError {
    #[error("Fs error: {0}")]
    Fs(#[from] FsError),
    #[error("Blobs store error: {0}")]
    Blobs(#[from] zim_core::blobs::BlobError),
    #[error("path is not in the published-set")]
    NotPublished,
    #[error("folder listings not yet supported via this endpoint")]
    FolderListingNotSupported,
}

impl IntoResponse for PublishedGetError {
    fn into_response(self) -> Response {
        match self {
            PublishedGetError::Fs(_) | PublishedGetError::Blobs(_) => {
                (http::StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
            }
            PublishedGetError::NotPublished => {
                (http::StatusCode::NOT_FOUND, self.to_string()).into_response()
            }
            PublishedGetError::FolderListingNotSupported => {
                (http::StatusCode::UNSUPPORTED_MEDIA_TYPE, self.to_string()).into_response()
            }
        }
    }
}
