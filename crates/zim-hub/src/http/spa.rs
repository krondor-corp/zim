//! Serves the Yew SPA (`web/dist`, embedded) at the site root `/`.
//!
//! Asset paths (the hashed `.js`/`.wasm` Trunk emits) resolve to embedded
//! files; any other path falls through to `index.html` so the client router
//! handles it. The SPA owns auth bootstrap (fetches `/api/v0/me`, redirects
//! to login on 401) and the web-key gate. Mounted as the hub's top-level
//! fallback, *after* `/api`, `/auth`, `/static`, … — so only unmatched paths
//! (`/`, `/v/:id`, `/settings`, deep links) reach it.

use axum::extract::OriginalUri;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct Spa;

fn index() -> Response {
    match Spa::get("index.html") {
        Some(file) => (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            file.data.into_owned(),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "SPA not built — run `trunk build` in crates/zim-hub/web",
        )
            .into_response(),
    }
}

fn asset(path: &str) -> Response {
    match Spa::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                [(header::CONTENT_TYPE, mime.as_ref())],
                file.data.into_owned(),
            )
                .into_response()
        }
        // Unknown path → the SPA's client router handles it.
        None => index(),
    }
}

/// `GET /` → the SPA shell.
pub async fn root() -> Response {
    index()
}

/// Top-level fallback: serve the embedded asset if it exists, else the SPA
/// shell so the client router can take over (deep links, client routes).
pub async fn fallback(OriginalUri(uri): OriginalUri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        index()
    } else {
        asset(path)
    }
}
