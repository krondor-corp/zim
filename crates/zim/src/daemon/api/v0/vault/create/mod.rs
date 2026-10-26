//! `POST /api/v0/vaults/create` — mint a new vault at genesis.
//!
//! A daemon paired with a hub seals the genesis manifest to every device
//! on the account (see [`roster`]) — the same answer a browser create
//! gives. Unpaired, or with `private`, it's sealed to this daemon alone.

mod roster;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_core::vault::VaultId;

use crate::daemon::api::client::ApiRequest;
use crate::ServiceState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRequest {
    pub name: String,
    /// Seal the vault to this daemon only, even when paired with a hub.
    #[serde(default)]
    pub private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateResponse {
    pub vault_id: VaultId,
    pub name: String,
    /// How many of your other devices the vault was sealed to at genesis.
    #[serde(default)]
    pub devices: usize,
}

pub async fn handler(
    State(state): State<ServiceState>,
    Json(request): Json<CreateRequest>,
) -> Result<impl IntoResponse, CreateError> {
    let shares = if request.private {
        Vec::new()
    } else {
        roster::account_devices(&state).await?
    };

    // The id is DERIVED from the genesis manifest's hash — minted by
    // `Vault::init_with_shares`, not chosen here. The log append makes
    // the vault discoverable by the coordinator's `open_vault`; no
    // registration step needed. The account's devices are sealed into
    // genesis itself — one head, nothing for the hub to conflict on.
    let vault = zim_peer::Vault::init_with_shares(
        request.name.clone(),
        state.peer().secret(),
        None,
        &shares,
        state.peer().coord().blobs().clone(),
        state.peer().coord().log().clone(),
    )
    .await
    .map_err(|e| CreateError::Init(e.to_string()))?;
    let vault_id = vault.id();
    // The owner's own key may be in the roster; it isn't an extra device.
    let devices = vault.manifest().shares().iter().count() - 1;

    // Tell every device sealed in at genesis that the vault exists, so it
    // shows up without waiting for the next reconcile sweep.
    if devices > 0 {
        let head = vault
            .head()
            .await
            .map_err(|e| CreateError::Init(e.to_string()))?;
        state.peer().announce_head(&vault, head).await;
    }

    Ok((
        http::StatusCode::OK,
        Json(CreateResponse {
            vault_id,
            name: request.name,
            devices,
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum CreateError {
    #[error(transparent)]
    Roster(#[from] roster::RosterError),
    #[error("vault init: {0}")]
    Init(String),
}

impl IntoResponse for CreateError {
    fn into_response(self) -> axum::response::Response {
        let status = match self {
            CreateError::Roster(roster::RosterError::Hub(_)) => http::StatusCode::BAD_GATEWAY,
            _ => http::StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, self.to_string()).into_response()
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
