mod views;

use axum::routing::get;
use axum::Router;

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/:bucket_id", get(views::tree::handler_root))
        .route("/:bucket_id/tree", get(views::tree::handler_root))
        .route("/:bucket_id/tree/*path", get(views::tree::handler))
        .route("/:bucket_id/blob/*path", get(views::blob::handler))
        .route("/:bucket_id/raw/*path", get(views::raw::handler))
        .route("/:bucket_id/history", get(views::history::handler))
        .with_state(state)
}
