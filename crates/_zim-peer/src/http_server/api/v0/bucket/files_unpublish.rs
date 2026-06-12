//! Per-file unpublication: `POST /api/v0/bucket/files/unpublish`.
//!
//! Removes the matching entry from
//! the manifest's `published_set`. Idempotent — no error if no such entry
//! exists. Note: unpublication does NOT re-encrypt; the per-blob secret can
//! still be used by anyone who already captured it. For true revocation,
//! use the rotate endpoint.

use axum::extract::{Json, State};
use axum::response::{IntoResponse, Response};
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zim_core::fs::FsError;

use crate::http_server::api::client::ApiRequest;
use crate::ServiceState;

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct FilesUnpublishRequest {
    /// Bucket ID
    #[arg(long)]
    pub bucket_id: Uuid,
    /// Display path of the file to unpublish (the value supplied at
    /// publish time, defaulting to the bucket path).
    #[arg(long)]
    pub display_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesUnpublishResponse {
    pub bucket_id: Uuid,
    pub display_path: String,
    pub removed: bool,
    pub new_bucket_link: String,
}

pub async fn handler(
    State(state): State<ServiceState>,
    Json(req): Json<FilesUnpublishRequest>,
) -> Result<impl IntoResponse, FilesUnpublishError> {
    tracing::info!(
        "FILES UNPUBLISH API: bucket={} display_path={}",
        req.bucket_id,
        req.display_path
    );

    let mut mount = state.peer().mount(req.bucket_id).await?;

    let our_key = state.peer().secret().public();
    {
        let inner = mount.inner().await;
        inner
            .manifest()
            .get_share(&our_key)
            .ok_or(FilesUnpublishError::NotOwner)?;
    }

    let abs_path = zim_core::fs::AbsPath::from_abs(std::path::PathBuf::from(&req.display_path));
    let removed = mount.unpublish(&abs_path).await;
    let new_bucket_link = state.peer().save_mount(&mount).await?;

    Ok((
        http::StatusCode::OK,
        Json(FilesUnpublishResponse {
            bucket_id: req.bucket_id,
            display_path: req.display_path,
            removed,
            new_bucket_link: new_bucket_link.hash().to_string(),
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum FilesUnpublishError {
    #[error("Fs error: {0}")]
    Fs(#[from] FsError),
    #[error("Only the bucket owner can unpublish files")]
    NotOwner,
}

impl IntoResponse for FilesUnpublishError {
    fn into_response(self) -> Response {
        match self {
            FilesUnpublishError::Fs(_) => (
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "Unexpected error".to_string(),
            )
                .into_response(),
            FilesUnpublishError::NotOwner => (
                http::StatusCode::FORBIDDEN,
                "Only the bucket owner can unpublish files".to_string(),
            )
                .into_response(),
        }
    }
}

impl ApiRequest for FilesUnpublishRequest {
    type Response = FilesUnpublishResponse;

    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let full_url = base_url.join("/api/v0/bucket/files/unpublish").unwrap();
        client.post(full_url).json(&self)
    }
}
