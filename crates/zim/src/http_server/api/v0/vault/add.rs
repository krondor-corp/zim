use std::io::Cursor;

use axum::response::IntoResponse;
use axum::Json;
use bytes::Bytes;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_core::fs::AbsPath;
use zim_core::linked_data::Link;

use axum::extract::State;

use crate::http_server::api::client::ApiRequest;
use crate::http_server::api::v0::vault::extractor::VaultHandle;
use crate::service_state::vault_lookup_response;
use crate::ServiceState;
use zim_peer::VaultLookupError;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddRequest {
    #[serde(skip)]
    pub vault_id: zim_core::vault::VaultId,
    pub path: String,
    pub bytes: Bytes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddResponse {
    pub path: String,
    pub link: Link,
    pub height: u64,
}

pub async fn handler(
    State(state): State<ServiceState>,
    VaultHandle { mut vault, .. }: VaultHandle,
    Json(req): Json<AddRequest>,
) -> Result<impl IntoResponse, AddError> {
    let abs = AbsPath::new(&req.path).ok_or_else(|| AddError::BadPath(req.path.clone()))?;
    vault
        .fs()
        .add(&abs, Cursor::new(req.bytes.to_vec()))
        .await
        .map_err(|e| AddError::Write(e.to_string()))?;
    let link = vault
        .save()
        .await
        .map_err(|e| AddError::Save(e.to_string()))?;
    let head = vault
        .head()
        .await
        .map_err(|e| AddError::Save(e.to_string()))?;
    state.peer().announce_head(&vault, head.clone()).await;
    Ok((
        http::StatusCode::OK,
        Json(AddResponse {
            path: req.path,
            link,
            height: head.height,
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum AddError {
    #[error(transparent)]
    Vault(#[from] VaultLookupError),
    #[error("invalid abs path: {0}")]
    BadPath(String),
    #[error("write: {0}")]
    Write(String),
    #[error("save: {0}")]
    Save(String),
}

impl IntoResponse for AddError {
    fn into_response(self) -> axum::response::Response {
        match self {
            AddError::Vault(e) => vault_lookup_response(e),
            AddError::BadPath(_) => {
                (http::StatusCode::BAD_REQUEST, self.to_string()).into_response()
            }
            _ => (http::StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response(),
        }
    }
}

impl ApiRequest for AddRequest {
    type Response = AddResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let url = base_url
            .join(&format!("/api/v0/vault/{}/add", self.vault_id))
            .unwrap();
        client.post(url).json(&self)
    }
}
