mod index;
mod static_files;

use axum::routing::get;
use axum::Router;

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(index::handler))
        .route("/static/*path", get(static_files::handler))
        .with_state(state)
}

// First action handler that returns a fragment vs. full page will add an
// `is_datastar(headers)` helper here, checking the `Datastar-Request` header.
