use axum::extract::{Json, State};
use axum::response::{IntoResponse, Response};
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zim_core::blobs::BlobStore;

use zim_core::fs::Manifest;
use zim_protocol::peer::sync::{DownloadPinsJob, SyncJob};

use crate::database::models::{BucketLogEntry, BucketStatus};
use crate::http_server::api::client::ApiRequest;
use crate::ServiceState;

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct ApproveRequest {
    /// Bucket ID to approve
    #[arg(long)]
    pub bucket_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproveResponse {
    pub bucket_id: Uuid,
    pub status: String,
}

pub async fn handler(
    State(state): State<ServiceState>,
    Json(req): Json<ApproveRequest>,
) -> Result<impl IntoResponse, ApproveError> {
    tracing::info!("APPROVE BUCKET: {}", req.bucket_id);

    // Check current status
    let current_status = BucketStatus::get_effective(&req.bucket_id, state.database())
        .map_err(|e| ApproveError::Database(e.to_string()))?;

    if current_status == BucketStatus::Active {
        return Ok((
            http::StatusCode::OK,
            Json(ApproveResponse {
                bucket_id: req.bucket_id,
                status: "active".to_string(),
            }),
        )
            .into_response());
    }

    // Set status to active
    BucketStatus::set(&req.bucket_id, BucketStatus::Active, None, state.database())
        .map_err(|e| ApproveError::Database(e.to_string()))?;

    // Trigger catch-up: download pins for all existing manifest entries
    let logs = BucketLogEntry::get_all(&req.bucket_id, state.database())
        .map_err(|e| ApproveError::Database(e.to_string()))?;

    for entry in logs {
        // Load manifest to get pins and peer list
        match state
            .peer()
            .blobs()
            .get_cbor::<Manifest>(&entry.current_link.hash())
            .await
        {
            Ok(manifest) => {
                let pins_link = manifest.pins().clone();
                let peer_ids = manifest.get_peer_ids();
                if let Err(e) = state
                    .peer()
                    .dispatch(SyncJob::DownloadPins(DownloadPinsJob {
                        pins_link,
                        peer_ids,
                    }))
                    .await
                {
                    tracing::warn!(
                        "Failed to dispatch catch-up pin download for height {}: {}",
                        entry.height,
                        e
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to load manifest at height {} for catch-up: {}",
                    entry.height,
                    e
                );
            }
        }
    }

    tracing::info!(
        "APPROVE BUCKET: {} approved and catch-up triggered",
        req.bucket_id
    );

    Ok((
        http::StatusCode::OK,
        Json(ApproveResponse {
            bucket_id: req.bucket_id,
            status: "active".to_string(),
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum ApproveError {
    #[error("Database error: {0}")]
    Database(String),
}

impl IntoResponse for ApproveError {
    fn into_response(self) -> Response {
        (http::StatusCode::INTERNAL_SERVER_ERROR, format!("{}", self)).into_response()
    }
}

impl ApiRequest for ApproveRequest {
    type Response = ApproveResponse;

    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let full_url = base_url.join("/api/v0/bucket/approve").unwrap();
        client.post(full_url).json(&self)
    }
}
