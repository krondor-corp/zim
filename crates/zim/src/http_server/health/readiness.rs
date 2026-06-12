//! `GET /_status/readyz` — readiness probe gated by [`DataSource`].

use std::time::Duration;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;

use super::data_source::{DataSourceError, StateDataSource};
use crate::http_server::api::client::ApiRequest;

const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadyzRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadyzResponse {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ApiRequest for ReadyzRequest {
    type Response = ReadyzResponse;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client.get(base.join("/_status/readyz").unwrap())
    }
}

#[tracing::instrument]
pub async fn handler(data: StateDataSource) -> Response {
    match timeout(HEALTH_CHECK_TIMEOUT, data.is_ready()).await {
        Ok(Ok(())) => (StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))).into_response(),
        Ok(Err(e)) => err_response(e),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "status": "failure",
                "message": "health check timed out"
            })),
        )
            .into_response(),
    }
}

fn err_response(err: DataSourceError) -> Response {
    let message = match err {
        DataSourceError::DependencyFailure => "one or more dependencies aren't available",
        DataSourceError::ShuttingDown => "service is shutting down",
    };
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({ "status": "failure", "message": message })),
    )
        .into_response()
}
