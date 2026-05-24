//! Start mount API endpoint

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::http_server::api::client::ApiRequest;
use crate::ServiceState;

/// Request to start a mount
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartMountRequest {
    pub mount_id: Uuid,
}

/// Response indicating mount was started
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartMountResponse {
    pub started: bool,
}

pub async fn handler(
    State(state): State<ServiceState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, StartMountError> {
    let mount_manager = state.mount_manager().read().await;
    let mount_manager = mount_manager
        .as_ref()
        .ok_or(StartMountError::MountManagerUnavailable)?;

    mount_manager.start(&id).await?;

    Ok((
        http::StatusCode::OK,
        Json(StartMountResponse { started: true }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum StartMountError {
    #[error("Fs manager unavailable")]
    MountManagerUnavailable,
    #[error("Fs error: {0}")]
    Fs(#[from] crate::fuse::FsError),
}

impl IntoResponse for StartMountError {
    fn into_response(self) -> Response {
        match self {
            StartMountError::MountManagerUnavailable => (
                http::StatusCode::SERVICE_UNAVAILABLE,
                "Fs manager not available",
            )
                .into_response(),
            StartMountError::Fs(e) => {
                (http::StatusCode::BAD_REQUEST, format!("Fs error: {}", e)).into_response()
            }
        }
    }
}

impl ApiRequest for StartMountRequest {
    type Response = StartMountResponse;

    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let full_url = base_url
            .join(&format!("/api/v0/mounts/{}/start", self.mount_id))
            .unwrap();
        client.post(full_url)
    }
}
