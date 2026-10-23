//! Keep browsers on the hub's canonical address (`HOST_NAME`).
//!
//! The hub's identity is a `did:web` fixed at startup, and the browser
//! side builds DIDs from the page's own origin. Reach a dev hub as
//! `localhost:17190` instead of `127.0.0.1:17190` and the browser asks
//! for `did:web:localhost…` while the hub's documents say
//! `did:web:127.0.0.1…` — resolution fails, and a web key minted there
//! would carry the wrong id. Session cookies are per host too.
//!
//! So a browser *page load* (GET/HEAD accepting HTML) on any other host
//! is redirected to the same path on `HOST_NAME`. API calls, daemons, and
//! `did.json` fetches are left alone.

use axum::extract::{Request, State};
use axum::http::{header, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};

use crate::state::AppState;

pub async fn redirect(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if let Some(target) = page_load_target(&req, &state.auth.host_name) {
        return Redirect::temporary(&target).into_response();
    }
    next.run(req).await
}

fn page_load_target(req: &Request, host_name: &str) -> Option<String> {
    let header = |name| req.headers().get(name).and_then(|v| v.to_str().ok());
    let is_page_load = matches!(*req.method(), Method::GET | Method::HEAD)
        && header(header::ACCEPT).is_some_and(|a| a.contains("text/html"));
    if !is_page_load {
        return None;
    }
    let path = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("/");
    target(header(header::HOST), host_name, path)
}

/// Where to send a page load that arrived with `Host: host`, or `None`
/// when it's already on `host_name` (or there's nothing to compare).
fn target(host: Option<&str>, host_name: &str, path: &str) -> Option<String> {
    let host = host?;
    let canonical = url::Url::parse(host_name).ok()?;
    let expected = match canonical.port() {
        Some(port) => format!("{}:{port}", canonical.host_str()?),
        None => canonical.host_str()?.to_string(),
    };
    if host.eq_ignore_ascii_case(&expected) {
        return None;
    }
    Some(format!("{}{path}", host_name.trim_end_matches('/')))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEV: &str = "http://127.0.0.1:17190";

    #[test]
    fn localhost_is_sent_to_the_canonical_dev_address() {
        assert_eq!(
            target(Some("localhost:17190"), DEV, "/v/abc?x=1").as_deref(),
            Some("http://127.0.0.1:17190/v/abc?x=1")
        );
    }

    #[test]
    fn the_canonical_address_is_left_alone() {
        assert_eq!(target(Some("127.0.0.1:17190"), DEV, "/"), None);
        assert_eq!(
            target(Some("Hub.Example.com"), "https://hub.example.com", "/"),
            None
        );
    }

    #[test]
    fn a_production_alias_goes_to_host_name() {
        assert_eq!(
            target(Some("www.hub.example.com"), "https://hub.example.com/", "/").as_deref(),
            Some("https://hub.example.com/")
        );
    }

    #[test]
    fn no_host_header_means_no_redirect() {
        assert_eq!(target(None, DEV, "/"), None);
    }
}
