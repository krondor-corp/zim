use axum::response::IntoResponse;
use axum::Json;
use bytes::Bytes;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_core::fs::AbsPath;

use crate::http_server::api::client::ApiRequest;
use crate::http_server::api::v0::vault::extractor::VaultHandle;
use crate::service_state::vault_lookup_response;
use zim_peer::VaultLookupError;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatRequest {
    #[serde(skip)]
    pub vault_id: zim_core::vault::VaultId,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatResponse {
    pub path: String,
    pub bytes: Bytes,
}

pub async fn handler(
    VaultHandle { vault, .. }: VaultHandle,
    Json(req): Json<CatRequest>,
) -> Result<impl IntoResponse, CatError> {
    let abs = AbsPath::new(&req.path).ok_or_else(|| CatError::BadPath(req.path.clone()))?;
    let bytes = vault
        .fs()
        .cat(&abs)
        .await
        .map_err(|e| CatError::Read(e.to_string()))?;
    Ok((
        http::StatusCode::OK,
        Json(CatResponse {
            path: req.path,
            bytes: Bytes::from(bytes),
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum CatError {
    #[error(transparent)]
    Vault(#[from] VaultLookupError),
    #[error("invalid abs path: {0}")]
    BadPath(String),
    #[error("read: {0}")]
    Read(String),
}

impl IntoResponse for CatError {
    fn into_response(self) -> axum::response::Response {
        match self {
            CatError::Vault(e) => vault_lookup_response(e),
            CatError::BadPath(_) => {
                (http::StatusCode::BAD_REQUEST, self.to_string()).into_response()
            }
            CatError::Read(_) => (http::StatusCode::NOT_FOUND, self.to_string()).into_response(),
        }
    }
}

impl ApiRequest for CatRequest {
    type Response = CatResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let url = base_url
            .join(&format!("/api/v0/vault/{}/cat", self.vault_id))
            .unwrap();
        client.post(url).json(&self)
    }
}
