//! Per-file publication: `POST /api/v0/bucket/files/publish`.
//!
//! Replaces whole-bucket publish — the bucket secret is never exposed.
//! Copies the named file's `Entry` into the manifest's
//! `published` map.

use std::path::PathBuf;

use axum::extract::{Json, State};
use axum::response::{IntoResponse, Response};
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zim_core::fs::FsError;

use crate::http_server::api::client::ApiRequest;
use crate::ServiceState;

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct FilesPublishRequest {
    /// Bucket ID
    #[arg(long)]
    pub bucket_id: Uuid,
    /// Path inside the bucket of the file to publish
    #[arg(long)]
    pub path: PathBuf,
    /// Optional display path the gateway serves this under
    /// (defaults to `path` itself).
    #[arg(long)]
    pub display_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesPublishResponse {
    pub bucket_id: Uuid,
    pub path: PathBuf,
    pub display_path: String,
    pub new_bucket_link: String,
}

pub async fn handler(
    State(state): State<ServiceState>,
    Json(req): Json<FilesPublishRequest>,
) -> Result<impl IntoResponse, FilesPublishError> {
    tracing::info!(
        "FILES PUBLISH API: bucket={} path={}",
        req.bucket_id,
        req.path.display()
    );

    let mut mount = state.peer().mount(req.bucket_id).await?;

    // Owner-only operation
    let our_key = state.peer().secret().public();
    {
        let inner = mount.inner().await;
        inner
            .manifest()
            .get_share(&our_key)
            .ok_or(FilesPublishError::NotOwner)?;
    }

    let abs_path = zim_core::fs::AbsPath::from_abs(req.path.clone());
    mount.publish(&abs_path).await?;
    let new_bucket_link = state.peer().save_mount(&mount).await?;

    let display_path = req
        .display_path
        .unwrap_or_else(|| req.path.to_string_lossy().into_owned());

    Ok((
        http::StatusCode::OK,
        Json(FilesPublishResponse {
            bucket_id: req.bucket_id,
            path: req.path,
            display_path,
            new_bucket_link: new_bucket_link.hash().to_string(),
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum FilesPublishError {
    #[error("Fs error: {0}")]
    Fs(#[from] FsError),
    #[error("Only the bucket owner can publish files")]
    NotOwner,
}

impl IntoResponse for FilesPublishError {
    fn into_response(self) -> Response {
        match self {
            FilesPublishError::Fs(_) => (
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "Unexpected error".to_string(),
            )
                .into_response(),
            FilesPublishError::NotOwner => (
                http::StatusCode::FORBIDDEN,
                "Only the bucket owner can publish files".to_string(),
            )
                .into_response(),
        }
    }
}

impl ApiRequest for FilesPublishRequest {
    type Response = FilesPublishResponse;

    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let full_url = base_url.join("/api/v0/bucket/files/publish").unwrap();
        client.post(full_url).json(&self)
    }
}
