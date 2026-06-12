//! `/api/v0/v/{vault_id}/...` — per-vault ciphertext + log endpoints.

pub mod blob;
pub mod head;
pub mod log;
pub mod manifest;

use axum::routing::get;
use axum::Router;

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/:vault_id/head", get(head::handler))
        .route("/:vault_id/log", get(log::handler))
        .route("/:vault_id/manifest", get(manifest::handler))
        .route("/:vault_id/blob/:hash", get(blob::handler))
        .with_state(state)
}
