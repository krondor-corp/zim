//! `/_admin` — admin panel, gated by `is_admin`.
//!
//! Single page (the dashboard) plus four POST actions for role
//! changes. All routes hit [`RequireAdmin`](crate::http::auth::RequireAdmin);
//! non-admin signed-in users get 403, anonymous users redirect to
//! login.
//!
//! The path is `_admin` (underscore) on purpose: it sits beside the
//! existing `/_status` + `/_events` "operator" namespace and makes
//! it obvious in logs that the request hit a privileged route.

mod actions;
mod dashboard;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    // Axum's `nest("/_admin", inner)` + `route("/", …)` makes the
    // dashboard reachable as bare `/_admin` (no trailing slash). The
    // `/_admin/` form is handled by a top-level redirect in
    // [`crate::http::build_router`] so browsers that auto-append a
    // slash don't 404.
    Router::new()
        .route("/", get(dashboard::handler))
        .route("/users/:id/authorize", post(actions::authorize))
        .route("/users/:id/unauthorize", post(actions::unauthorize))
        .route("/users/:id/promote", post(actions::promote))
        .route("/users/:id/demote", post(actions::demote))
        .with_state(state)
}
