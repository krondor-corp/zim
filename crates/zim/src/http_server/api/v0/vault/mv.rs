use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_core::fs::AbsPath;
use zim_core::linked_data::Link;

use crate::http_server::api::client::ApiRequest;
use crate::http_server::api::v0::vault::extractor::VaultHandle;
use crate::service_state::vault_lookup_response;
use crate::ServiceState;
use zim_peer::VaultLookupError;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MvRequest {
    #[serde(skip)]
    pub vault_id: zim_core::vault::VaultId,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MvResponse {
    pub from: String,
    pub to: String,
    pub link: Link,
    pub height: u64,
}

pub async fn handler(
    State(state): State<ServiceState>,
    VaultHandle { mut vault, .. }: VaultHandle,
    Json(req): Json<MvRequest>,
) -> Result<impl IntoResponse, MvError> {
    let from = AbsPath::new(&req.from).ok_or_else(|| MvError::BadPath(req.from.clone()))?;
    let to = AbsPath::new(&req.to).ok_or_else(|| MvError::BadPath(req.to.clone()))?;
    vault
        .fs()
        .mv(&from, &to)
        .await
        .map_err(|e| MvError::Mv(e.to_string()))?;
    let link = vault
        .save()
        .await
        .map_err(|e| MvError::Save(e.to_string()))?;
    let head = vault
        .head()
        .await
        .map_err(|e| MvError::Save(e.to_string()))?;
    state.peer().announce_head(&vault, head.clone()).await;
    Ok((
        http::StatusCode::OK,
        Json(MvResponse {
            from: req.from,
            to: req.to,
            link,
            height: head.height,
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum MvError {
    #[error(transparent)]
    Vault(#[from] VaultLookupError),
    #[error("invalid abs path: {0}")]
    BadPath(String),
    #[error("mv: {0}")]
    Mv(String),
    #[error("save: {0}")]
    Save(String),
}

impl IntoResponse for MvError {
    fn into_response(self) -> axum::response::Response {
        match self {
            MvError::Vault(e) => vault_lookup_response(e),
            MvError::BadPath(_) => {
                (http::StatusCode::BAD_REQUEST, self.to_string()).into_response()
            }
            _ => (http::StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response(),
        }
    }
}

impl ApiRequest for MvRequest {
    type Response = MvResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let url = base_url
            .join(&format!("/api/v0/vault/{}/mv", self.vault_id))
            .unwrap();
        client.post(url).json(&self)
    }
}
