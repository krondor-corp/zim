use axum::extract::{Json, Query, State};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zim_core::fs::AbsPath;

use zim_core::fs::FsError;

use crate::http_server::api::client::ApiRequest;
use crate::ServiceState;

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct CatRequest {
    /// Bucket ID to read from
    #[arg(long)]
    pub bucket_id: Uuid,

    /// Path in bucket to read
    #[arg(long)]
    pub path: String,

    /// Optional: specific version hash to read from
    #[arg(long)]
    #[serde(default)]
    pub at: Option<String>,

    /// Optional: force download (attachment) instead of inline display
    #[arg(long)]
    #[serde(default)]
    pub download: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatResponse {
    pub path: String,
    /// Base64-encoded file content
    pub content: String,
    pub size: usize,
    pub mime_type: String,
}

// JSON POST handler (original)
pub async fn handler(
    State(state): State<ServiceState>,
    Json(req): Json<CatRequest>,
) -> Result<impl IntoResponse, CatError> {
    let response = handle_cat_request(state, req).await?;
    Ok((http::StatusCode::OK, Json(response)).into_response())
}

// Query GET handler (for viewing/downloading)
pub async fn handler_get(
    State(state): State<ServiceState>,
    Query(req): Query<CatRequest>,
) -> Result<Response, CatError> {
    let is_download = req.download.unwrap_or(false);
    let cat_response = handle_cat_request(state, req).await?;

    // Decode base64 content back to bytes
    let content_bytes = base64::engine::general_purpose::STANDARD
        .decode(&cat_response.content)
        .map_err(|e| CatError::InvalidPath(format!("Failed to decode content: {}", e)))?;

    // Determine Content-Disposition header (inline for viewing, attachment for download)
    let disposition = if is_download {
        format!(
            "attachment; filename=\"{}\"",
            std::path::Path::new(&cat_response.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("download")
        )
    } else {
        format!(
            "inline; filename=\"{}\"",
            std::path::Path::new(&cat_response.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
        )
    };

    // Return as binary with appropriate headers
    Ok((
        http::StatusCode::OK,
        [
            (
                axum::http::header::CONTENT_TYPE,
                cat_response.mime_type.as_str(),
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                disposition.as_str(),
            ),
        ],
        content_bytes,
    )
        .into_response())
}

async fn handle_cat_request(state: ServiceState, req: CatRequest) -> Result<CatResponse, CatError> {
    // Load mount - either from specific link or role-based
    let mount = if let Some(hash_str) = &req.at {
        // Parse the hash string and create a Link
        match hash_str.parse::<zim_core::linked_data::Hash>() {
            Ok(hash) => {
                let link =
                    zim_core::linked_data::Link::new(zim_core::linked_data::LD_RAW_CODEC, hash);
                match zim_core::fs::Fs::<zim_core::blobs::BlobsStore>::load(
                    &link,
                    state.peer().secret(),
                    state.peer().blobs(),
                )
                .await
                {
                    Ok(mount) => mount,
                    Err(e) => {
                        tracing::error!("Failed to load mount from link: {}", e);
                        return Err(CatError::Fs(e));
                    }
                }
            }
            Err(e) => {
                return Err(CatError::InvalidPath(format!("Invalid hash format: {}", e)));
            }
        }
    } else {
        // Load mount based on role (owners see HEAD, mirrors see latest_published)
        state.peer().mount_for_read(req.bucket_id).await?
    };

    let path_buf = std::path::PathBuf::from(&req.path);
    if !path_buf.is_absolute() {
        return Err(CatError::InvalidPath("Path must be absolute".into()));
    }

    // Get file data
    let data = mount.cat(&AbsPath::from_abs(path_buf.clone())).await?;

    // Get leaf to extract MIME type
    let leaf = mount
        .get_leaf_at_path(&AbsPath::from_abs(path_buf.clone()))
        .await?
        .ok_or_else(|| {
            zim_core::fs::FsError::PathNotFound(AbsPath::from_abs(path_buf.clone()))
        })?;
    let mime_type = leaf
        .mime()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    // Encode as base64 for JSON transport
    let content = base64::engine::general_purpose::STANDARD.encode(&data);
    let size = data.len();

    Ok(CatResponse {
        path: req.path,
        content,
        size,
        mime_type,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum CatError {
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    #[error("Fs error: {0}")]
    Fs(#[from] FsError),
}

impl IntoResponse for CatError {
    fn into_response(self) -> Response {
        match self {
            CatError::InvalidPath(msg) => (
                http::StatusCode::BAD_REQUEST,
                format!("Invalid path: {}", msg),
            )
                .into_response(),
            CatError::Fs(_) => (
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "Unexpected error".to_string(),
            )
                .into_response(),
        }
    }
}

// Client implementation - builds request for this operation
impl ApiRequest for CatRequest {
    type Response = CatResponse;

    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let full_url = base_url.join("/api/v0/bucket/cat").unwrap();
        client.post(full_url).json(&self)
    }
}
