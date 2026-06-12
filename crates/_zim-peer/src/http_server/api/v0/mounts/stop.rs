//! Stop mount API endpoint

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::http_server::api::client::ApiRequest;
use crate::ServiceState;

/// Request to stop a mount
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopMountRequest {
    pub mount_id: Uuid,
}

/// Response indicating mount was stopped
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopMountResponse {
    pub stopped: bool,
}

pub async fn handler(
    State(state): State<ServiceState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, StopMountError> {
    let mount_manager = state.mount_manager().read().await;
    let mount_manager = mount_manager
        .as_ref()
        .ok_or(StopMountError::MountManagerUnavailable)?;

    mount_manager.stop(&id).await?;

    Ok((
        http::StatusCode::OK,
        Json(StopMountResponse { stopped: true }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum StopMountError {
    #[error("Fs manager unavailable")]
    MountManagerUnavailable,
    #[error("Fs error: {0}")]
    Fs(#[from] crate::fuse::FsError),
}

impl IntoResponse for StopMountError {
    fn into_response(self) -> Response {
        match self {
            StopMountError::MountManagerUnavailable => (
                http::StatusCode::SERVICE_UNAVAILABLE,
                "Fs manager not available",
            )
                .into_response(),
            StopMountError::Fs(e) => {
                (http::StatusCode::BAD_REQUEST, format!("Fs error: {}", e)).into_response()
            }
        }
    }
}

impl ApiRequest for StopMountRequest {
    type Response = StopMountResponse;

    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let full_url = base_url
            .join(&format!("/api/v0/mounts/{}/stop", self.mount_id))
            .unwrap();
        client.post(full_url)
    }
}
