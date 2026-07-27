//! `GET /_status/identity` — the peer's public key.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};

use crate::daemon::api::client::ApiRequest;
use crate::daemon::state::ServiceState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityResponse {
    /// Hex-encoded Ed25519 public key.
    pub node_id: String,
}

impl ApiRequest for IdentityRequest {
    type Response = IdentityResponse;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client.get(base.join("/_status/identity").unwrap())
    }
}

#[tracing::instrument(skip(state))]
pub async fn handler(State(state): State<ServiceState>) -> Response {
    let node_id = state.peer().id().to_hex();
    (StatusCode::OK, Json(IdentityResponse { node_id })).into_response()
}
