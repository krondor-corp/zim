//! Per-folder publication: `POST /api/v0/bucket/folders/publish`.
//!
//! Marks the named directory's body as gateway-readable. The hub can then
//! walk descendants using each child `Entry` secret. Bucket secret is
//! never exposed.

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
pub struct FoldersPublishRequest {
    #[arg(long)]
    pub bucket_id: Uuid,
    #[arg(long)]
    pub path: PathBuf,
    #[arg(long)]
    pub display_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldersPublishResponse {
    pub bucket_id: Uuid,
    pub path: PathBuf,
    pub display_path: String,
    pub new_bucket_link: String,
}

pub async fn handler(
    State(state): State<ServiceState>,
    Json(req): Json<FoldersPublishRequest>,
) -> Result<impl IntoResponse, FoldersPublishError> {
    tracing::info!(
        "FOLDERS PUBLISH API: bucket={} path={}",
        req.bucket_id,
        req.path.display()
    );

    let mut mount = state.peer().mount(req.bucket_id).await?;

    let our_key = state.peer().secret().public();
    {
        let inner = mount.inner().await;
        inner
            .manifest()
            .get_share(&our_key)
            .ok_or(FoldersPublishError::NotOwner)?;
    }

    let abs_path = zim_core::fs::AbsPath::from_abs(req.path.clone());
    mount.publish(&abs_path).await?;
    let new_bucket_link = state.peer().save_mount(&mount).await?;

    let display_path = req
        .display_path
        .unwrap_or_else(|| req.path.to_string_lossy().into_owned());

    Ok((
        http::StatusCode::OK,
        Json(FoldersPublishResponse {
            bucket_id: req.bucket_id,
            path: req.path,
            display_path,
            new_bucket_link: new_bucket_link.hash().to_string(),
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum FoldersPublishError {
    #[error("Fs error: {0}")]
    Fs(#[from] FsError),
    #[error("Only the bucket owner can publish folders")]
    NotOwner,
}

impl IntoResponse for FoldersPublishError {
    fn into_response(self) -> Response {
        match self {
            FoldersPublishError::Fs(_) => (
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "Unexpected error".to_string(),
            )
                .into_response(),
            FoldersPublishError::NotOwner => (
                http::StatusCode::FORBIDDEN,
                "Only the bucket owner can publish folders".to_string(),
            )
                .into_response(),
        }
    }
}

impl ApiRequest for FoldersPublishRequest {
    type Response = FoldersPublishResponse;

    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let full_url = base_url.join("/api/v0/bucket/folders/publish").unwrap();
        client.post(full_url).json(&self)
    }
}
