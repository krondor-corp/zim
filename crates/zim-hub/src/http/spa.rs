//! Serves the Yew SPA (`web/dist`, embedded) at the site root `/`.
//!
//! Asset paths (the hashed `.js`/`.wasm` Trunk emits) resolve to embedded
//! files, public; any other path falls through to `index.html` so the
//! client router handles it — but only for a signed-in, approved user.
//! Auth stays out of the wasm app: anonymous visitors are sent to the
//! server-rendered `/auth/login` (returning here via `next`), pending ones
//! to `/auth/pending`. The app owns the web-key gate and everything after.
//! Mounted as the hub's top-level fallback, *after* `/api`, `/auth`,
//! `/static`, … — so only unmatched paths (`/`, `/v/:id`, `/settings`,
//! deep links) reach it.

use axum::extract::OriginalUri;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Redirect, Response};
use rust_embed::RustEmbed;

use crate::database::models::User;
use crate::http::auth::{return_to, OptionalUser, PENDING_PATH};

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

/// `index.html` for users who may use the app; everyone else to the
/// right `/auth/*` page.
fn gated_index(user: Option<User>, uri: &Uri) -> Response {
    match user {
        None => {
            let here = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
            Redirect::to(&return_to::login_url(here)).into_response()
        }
        Some(u) if !u.can_use_app() => Redirect::to(PENDING_PATH).into_response(),
        Some(_) => index(),
    }
}

fn asset(path: &str) -> Option<Response> {
    Spa::get(path).map(|file| {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        (
            [(header::CONTENT_TYPE, mime.as_ref())],
            file.data.into_owned(),
        )
            .into_response()
    })
}

/// `GET /` → the SPA shell (gated).
pub async fn root(OptionalUser(user): OptionalUser, OriginalUri(uri): OriginalUri) -> Response {
    gated_index(user, &uri)
}

/// Top-level fallback: serve the embedded asset if it exists, else the
/// (gated) SPA shell so the client router can take over (deep links,
/// client routes).
pub async fn fallback(OptionalUser(user): OptionalUser, OriginalUri(uri): OriginalUri) -> Response {
    let path = uri.path().trim_start_matches('/');
    // `index.html` is the app itself, not a public asset.
    if !path.is_empty() && path != "index.html" {
        if let Some(file) = asset(path) {
            return file;
        }
    }
    gated_index(user, &uri)
}
