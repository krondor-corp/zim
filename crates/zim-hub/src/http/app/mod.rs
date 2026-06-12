//! `/app/*` — the authenticated workspace.
//!
//! Everything that reads user data lives behind this prefix.
//! Marketing (`/`), auth (`/auth/*`), and operator infra
//! (`/_status`, etc.) stay outside.

mod onboarding;
mod workspace;

use axum::routing::get;
use axum::Router;

use crate::http::admin;
use crate::http::html::{peers, vault};
use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        // Workspace landing. Uses `RequireOnboardedUser`, so a fresh
        // user with zero devices is auto-redirected to the
        // onboarding page below.
        .route("/", get(workspace::handler))
        // Per-vault tree views (browser-side decrypt UI). Onboarded.
        .nest("/v", vault::router(state.clone()))
        // Forced first-run setup. Reachable *without* a device since
        // it's the page that lets you add one.
        .route("/onboarding", get(onboarding::handler))
        // Self-serve device management. Also pre-onboarding-reachable
        // — finishing onboarding means landing your first row here.
        .nest("/devices", peers::router(state.clone()))
        // Old `/app/peers` bookmarks → permanent redirect to the
        // renamed page.
        .route(
            "/peers",
            get(|| async { axum::response::Redirect::permanent("/app/devices") }),
        )
        // Admin panel — admin role implicitly bypasses the
        // onboarding gate (see `RequireOnboardedUser`).
        .nest("/_admin", admin::router(state.clone()))
        .route(
            "/_admin/",
            get(|| async { axum::response::Redirect::permanent("/app/_admin") }),
        )
        .with_state(state)
}
