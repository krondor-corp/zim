use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_core::fs::AbsPath;
use zim_core::linked_data::Link;

use crate::daemon::api::client::ApiRequest;
use crate::daemon::api::v0::vault::extractor::VaultHandle;
use crate::daemon::state::vault_lookup_response;
use crate::ServiceState;
use zim_peer::VaultLookupError;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RmRequest {
    #[serde(skip)]
    pub vault_id: zim_core::vault::VaultId,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RmResponse {
    pub path: String,
    pub link: Link,
    pub height: u64,
}

pub async fn handler(
    State(state): State<ServiceState>,
    VaultHandle { id, mut vault }: VaultHandle,
    Json(req): Json<RmRequest>,
) -> Result<impl IntoResponse, RmError> {
    #[cfg(feature = "fuse")]
    if let Some(res) = state.mounts().fs_rm(id, req.path.clone()).await {
        let c = res.map_err(|e| RmError::Rm(e.to_string()))?;
        return Ok((
            http::StatusCode::OK,
            Json(RmResponse {
                path: req.path,
                link: c.link,
                height: c.height,
            }),
        )
            .into_response());
    }
    #[cfg(not(feature = "fuse"))]
    let _ = id;

    let abs = AbsPath::new(&req.path).ok_or_else(|| RmError::BadPath(req.path.clone()))?;
    vault
        .fs()
        .rm(&abs)
        .await
        .map_err(|e| RmError::Rm(e.to_string()))?;
    let link = vault
        .save()
        .await
        .map_err(|e| RmError::Save(e.to_string()))?;
    let head = vault
        .head()
        .await
        .map_err(|e| RmError::Save(e.to_string()))?;
    state.peer().announce_head(&vault, head.clone()).await;
    Ok((
        http::StatusCode::OK,
        Json(RmResponse {
            path: req.path,
            link,
            height: head.height,
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum RmError {
    #[error(transparent)]
    Vault(#[from] VaultLookupError),
    #[error("invalid abs path: {0}")]
    BadPath(String),
    #[error("rm: {0}")]
    Rm(String),
    #[error("save: {0}")]
    Save(String),
}

impl IntoResponse for RmError {
    fn into_response(self) -> axum::response::Response {
        match self {
            RmError::Vault(e) => vault_lookup_response(e),
            RmError::BadPath(_) => {
                (http::StatusCode::BAD_REQUEST, self.to_string()).into_response()
            }
            _ => (http::StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response(),
        }
    }
}

impl ApiRequest for RmRequest {
    type Response = RmResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let url = base_url
            .join(&format!("/api/v0/vaults/{}/rm", self.vault_id))
            .unwrap();
        client.post(url).json(&self)
    }
}
