//! Per-folder unpublication: `POST /api/v0/bucket/folders/unpublish`.

use axum::extract::{Json, State};
use axum::response::{IntoResponse, Response};
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zim_core::fs::FsError;

use crate::http_server::api::client::ApiRequest;
use crate::ServiceState;

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct FoldersUnpublishRequest {
    #[arg(long)]
    pub bucket_id: Uuid,
    #[arg(long)]
    pub display_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldersUnpublishResponse {
    pub bucket_id: Uuid,
    pub display_path: String,
    pub removed: bool,
    pub new_bucket_link: String,
}

pub async fn handler(
    State(state): State<ServiceState>,
    Json(req): Json<FoldersUnpublishRequest>,
) -> Result<impl IntoResponse, FoldersUnpublishError> {
    tracing::info!(
        "FOLDERS UNPUBLISH API: bucket={} display_path={}",
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
            .ok_or(FoldersUnpublishError::NotOwner)?;
    }

    let abs_path = zim_core::fs::AbsPath::from_abs(std::path::PathBuf::from(&req.display_path));
    let removed = mount.unpublish(&abs_path).await;
    let new_bucket_link = state.peer().save_mount(&mount).await?;

    Ok((
        http::StatusCode::OK,
        Json(FoldersUnpublishResponse {
            bucket_id: req.bucket_id,
            display_path: req.display_path,
            removed,
            new_bucket_link: new_bucket_link.hash().to_string(),
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum FoldersUnpublishError {
    #[error("Fs error: {0}")]
    Fs(#[from] FsError),
    #[error("Only the bucket owner can unpublish folders")]
    NotOwner,
}

impl IntoResponse for FoldersUnpublishError {
    fn into_response(self) -> Response {
        match self {
            FoldersUnpublishError::Fs(_) => (
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "Unexpected error".to_string(),
            )
                .into_response(),
            FoldersUnpublishError::NotOwner => (
                http::StatusCode::FORBIDDEN,
                "Only the bucket owner can unpublish folders".to_string(),
            )
                .into_response(),
        }
    }
}

impl ApiRequest for FoldersUnpublishRequest {
    type Response = FoldersUnpublishResponse;

    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let full_url = base_url.join("/api/v0/bucket/folders/unpublish").unwrap();
        client.post(full_url).json(&self)
    }
}
