//! Delete mount API endpoint

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::http_server::api::client::ApiRequest;
use crate::ServiceState;

/// Request to delete a mount
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteMountRequest {
    pub mount_id: Uuid,
}

/// Response indicating mount was deleted
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteMountResponse {
    pub deleted: bool,
}

pub async fn handler(
    State(state): State<ServiceState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, DeleteMountError> {
    let mount_manager = state.mount_manager().read().await;
    let mount_manager = mount_manager
        .as_ref()
        .ok_or(DeleteMountError::MountManagerUnavailable)?;

    let deleted = mount_manager.delete(&id).await?;

    Ok((http::StatusCode::OK, Json(DeleteMountResponse { deleted })).into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum DeleteMountError {
    #[error("Fs manager unavailable")]
    MountManagerUnavailable,
    #[error("Fs error: {0}")]
    Fs(#[from] crate::fuse::FsError),
}

impl IntoResponse for DeleteMountError {
    fn into_response(self) -> Response {
        match self {
            DeleteMountError::MountManagerUnavailable => (
                http::StatusCode::SERVICE_UNAVAILABLE,
                "Fs manager not available",
            )
                .into_response(),
            DeleteMountError::Fs(e) => (
                http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Fs error: {}", e),
            )
                .into_response(),
        }
    }
}

impl ApiRequest for DeleteMountRequest {
    type Response = DeleteMountResponse;

    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let full_url = base_url
            .join(&format!("/api/v0/mounts/{}", self.mount_id))
            .unwrap();
        client.delete(full_url)
    }
}
