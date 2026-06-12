use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_core::vault::VaultId;

use crate::http_server::api::client::ApiRequest;
use crate::ServiceState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateResponse {
    pub vault_id: VaultId,
    pub name: String,
}

pub async fn handler(
    State(state): State<ServiceState>,
    Json(request): Json<CreateRequest>,
) -> Result<impl IntoResponse, CreateError> {
    // The id is DERIVED from the genesis manifest's hash — minted by
    // `Vault::init`, not chosen here. Vault::init's log append makes
    // the vault discoverable by the coordinator's `open_vault`; no
    // registration step needed.
    let vault = zim_peer::Vault::init(
        request.name.clone(),
        state.peer().secret(),
        state.peer().coord().blobs().clone(),
        state.peer().coord().log().clone(),
    )
    .await
    .map_err(|e| CreateError::Init(e.to_string()))?;
    let vault_id = vault.id();

    Ok((
        http::StatusCode::OK,
        Json(CreateResponse {
            vault_id,
            name: request.name,
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum CreateError {
    #[error("vault init: {0}")]
    Init(String),
}

impl IntoResponse for CreateError {
    fn into_response(self) -> axum::response::Response {
        (http::StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}

impl ApiRequest for CreateRequest {
    type Response = CreateResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        client
            .post(base_url.join("/api/v0/vaults/create").unwrap())
            .json(&self)
    }
}
