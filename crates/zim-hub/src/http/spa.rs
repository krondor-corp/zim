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
//!
//! Serving `index.html` also hands the app its timings: the hub's
//! [`ClientConfig`] goes in as a `<script type="application/json">` before
//! `</head>`, so the page needs no extra round trip to learn them. A JSON
//! script is data, never executed, so the CSP is unchanged.

use axum::extract::{OriginalUri, State};
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Redirect, Response};
use rust_embed::RustEmbed;
use zim_api::hub::{ClientConfig, CLIENT_CONFIG_ELEMENT_ID};

use crate::database::models::User;
use crate::http::auth::{return_to, OptionalUser, PENDING_PATH};
use crate::state::AppState;

#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct Spa;

fn index(client: &ClientConfig) -> Response {
    match Spa::get("index.html") {
        Some(file) => (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            with_client_config(&String::from_utf8_lossy(&file.data), client),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "SPA not built — run `trunk build` in crates/zim-hub/web",
        )
            .into_response(),
    }
}

/// `html` with `client` injected as the `#zim-config` JSON script, just
/// before `</head>` (appended if the page has none). The JSON is only
/// numbers, so it can't close the script tag early.
fn with_client_config(html: &str, client: &ClientConfig) -> String {
    let json = serde_json::to_string(client).expect("ClientConfig serializes");
    let tag = format!(
        r#"<script type="application/json" id="{CLIENT_CONFIG_ELEMENT_ID}">{json}</script>"#
    );
    match html.find("</head>") {
        Some(at) => format!("{}{tag}{}", &html[..at], &html[at..]),
        None => format!("{html}{tag}"),
    }
}

/// `index.html` for users who may use the app; everyone else to the
/// right `/auth/*` page.
fn gated_index(user: Option<User>, uri: &Uri, client: &ClientConfig) -> Response {
    match user {
        None => {
            let here = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
            Redirect::to(&return_to::login_url(here)).into_response()
        }
        Some(u) if !u.can_use_app() => Redirect::to(PENDING_PATH).into_response(),
        Some(_) => index(client),
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
pub async fn root(
    State(state): State<AppState>,
    OptionalUser(user): OptionalUser,
    OriginalUri(uri): OriginalUri,
) -> Response {
    gated_index(user, &uri, &state.client)
}

/// Top-level fallback: serve the embedded asset if it exists, else the
/// (gated) SPA shell so the client router can take over (deep links,
/// client routes).
pub async fn fallback(
    State(state): State<AppState>,
    OptionalUser(user): OptionalUser,
    OriginalUri(uri): OriginalUri,
) -> Response {
    let path = uri.path().trim_start_matches('/');
    // `index.html` is the app itself, not a public asset.
    if !path.is_empty() && path != "index.html" {
        if let Some(file) = asset(path) {
            return file;
        }
    }
    gated_index(user, &uri, &state.client)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The page Trunk emits, trimmed to what matters here.
    const TRUNK_INDEX: &str =
        "<!doctype html><html><head><title>zim</title></head><body></body></html>";

    /// The config the browser would read back out of the served page.
    fn read_back(page: &str) -> ClientConfig {
        let open = format!(r#"<script type="application/json" id="{CLIENT_CONFIG_ELEMENT_ID}">"#);
        let start = page.find(&open).expect("config tag present") + open.len();
        let len = page[start..].find("</script>").expect("config tag closed");
        serde_json::from_str(&page[start..start + len]).expect("config is valid JSON")
    }

    #[test]
    fn alice_dev_hub_hands_her_browser_fast_timings() {
        // Alice runs `make hub`, which shortens the SPA's timers.
        let dev = ClientConfig {
            autosave_ms: 1_500,
            follow_ms: 1_000,
        };

        // Her browser loads the app shell.
        let page = with_client_config(TRUNK_INDEX, &dev);

        // The timings ride along in the head, ahead of the body the app
        // mounts into — and read back exactly as the hub set them.
        let tag = page.find(CLIENT_CONFIG_ELEMENT_ID).unwrap();
        assert!(tag < page.find("</head>").unwrap());
        assert_eq!(read_back(&page), dev);
        assert!(page.ends_with("</head><body></body></html>"));
    }

    #[test]
    fn bob_production_hub_hands_out_the_default_timings() {
        // Bob's hub sets no timing env vars.
        let page = with_client_config(TRUNK_INDEX, &ClientConfig::default());

        // His browser autosaves every 30s and follows every 3s, as before.
        assert_eq!(
            read_back(&page),
            ClientConfig {
                autosave_ms: 30_000,
                follow_ms: 3_000,
            }
        );
    }

    #[test]
    fn a_page_without_a_head_still_carries_the_config() {
        let page = with_client_config("<body></body>", &ClientConfig::default());
        assert_eq!(read_back(&page), ClientConfig::default());
    }
}
