//! `POST /api/v0/peers/reconcile` — fold trusted contacts into the
//! vaults this daemon authored.
//!
//! Drives [`zim_peer::Peer::reconcile_trusted`]: every trusted contact's
//! resolved device keys are granted access to each owned vault, the new
//! heads announced. Idempotent — nothing to add means no chain advance.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};

use crate::http_server::api::client::ApiRequest;
use crate::ServiceState;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReconcileRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileResponse {
    pub vaults_scanned: usize,
    pub vaults_updated: usize,
    pub shares_added: usize,
}

pub async fn handler(
    State(state): State<ServiceState>,
    Json(_req): Json<ReconcileRequest>,
) -> Result<impl IntoResponse, ReconcileError> {
    let report =
        crate::reconcile::reconcile_trusted(state.peer(), state.peers(), state.resolver().as_ref())
            .await
            .map_err(|e| ReconcileError::Reconcile(e.to_string()))?;
    Ok((
        http::StatusCode::OK,
        Json(ReconcileResponse {
            vaults_scanned: report.vaults_scanned,
            vaults_updated: report.vaults_updated,
            shares_added: report.shares_added,
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum ReconcileError {
    #[error("reconcile: {0}")]
    Reconcile(String),
}

impl IntoResponse for ReconcileError {
    fn into_response(self) -> axum::response::Response {
        (http::StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}

impl ApiRequest for ReconcileRequest {
    type Response = ReconcileResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        client
            .post(base_url.join("/api/v0/peers/reconcile").unwrap())
            .json(&self)
    }
}
