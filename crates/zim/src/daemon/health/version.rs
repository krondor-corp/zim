//! `GET /_status/version` — returns the daemon's [`BuildInfo`].

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};

use crate::daemon::api::client::ApiRequest;
use crate::version::{build_info, BuildInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionRequest {}

impl ApiRequest for VersionRequest {
    type Response = BuildInfo;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client.get(base.join("/_status/version").unwrap())
    }
}

#[tracing::instrument]
pub async fn handler() -> Response {
    (StatusCode::OK, Json(build_info())).into_response()
}
