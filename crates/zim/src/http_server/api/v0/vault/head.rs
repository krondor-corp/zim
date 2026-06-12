use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_core::linked_data::Link;
use zim_core::vault::VaultId;

use crate::http_server::api::client::ApiRequest;
use crate::http_server::api::v0::vault::extractor::VaultHandle;
use crate::service_state::vault_lookup_response;
use zim_peer::VaultLookupError;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HeadRequest {
    /// Path-param; never serialised in the body.
    #[serde(skip)]
    pub vault_id: VaultId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadResponse {
    pub vault_id: VaultId,
    pub link: Link,
    pub height: u64,
}

pub async fn handler(
    VaultHandle { id, vault }: VaultHandle,
    Json(_req): Json<HeadRequest>,
) -> Result<impl IntoResponse, HeadError> {
    let head = vault
        .head()
        .await
        .map_err(|e| HeadError::Load(e.to_string()))?;
    Ok((
        http::StatusCode::OK,
        Json(HeadResponse {
            vault_id: id,
            link: head.link,
            height: head.height,
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum HeadError {
    #[error(transparent)]
    Vault(#[from] VaultLookupError),
    #[error("load: {0}")]
    Load(String),
}

impl IntoResponse for HeadError {
    fn into_response(self) -> axum::response::Response {
        match self {
            HeadError::Vault(e) => vault_lookup_response(e),
            HeadError::Load(_) => {
                (http::StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
            }
        }
    }
}

impl ApiRequest for HeadRequest {
    type Response = HeadResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let url = base_url
            .join(&format!("/api/v0/vault/{}/head", self.vault_id))
            .unwrap();
        client.post(url).json(&self)
    }
}
