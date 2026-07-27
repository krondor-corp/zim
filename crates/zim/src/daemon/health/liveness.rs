//! `GET /_status/livez` — minimum-viable liveness probe.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};

use crate::daemon::api::client::ApiRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivezRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivezResponse {
    pub status: String,
}

impl ApiRequest for LivezRequest {
    type Response = LivezResponse;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client.get(base.join("/_status/livez").unwrap())
    }
}

#[tracing::instrument]
pub async fn handler() -> Response {
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))).into_response()
}
