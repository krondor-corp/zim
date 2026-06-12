//! Content-negotiated 404 fallback. JSON for JSON callers, text
//! otherwise. Mirrors `_zim-peer`'s `handlers/not_found.rs`.

use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

pub async fn handler(headers: HeaderMap) -> Response {
    let accept = headers
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok());
    match accept {
        Some(accept_str) if accept_str.contains("application/json") => {
            let body = serde_json::json!({ "msg": "not found" });
            (StatusCode::NOT_FOUND, Json(body)).into_response()
        }
        _ => (
            StatusCode::NOT_FOUND,
            [(axum::http::header::CONTENT_TYPE, "text/plain")],
            "not found",
        )
            .into_response(),
    }
}
