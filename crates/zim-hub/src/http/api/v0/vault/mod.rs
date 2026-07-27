//! `/api/v0/vaults/{vault_id}/...` — per-vault ciphertext + log endpoints.

pub mod head;
pub mod log;
pub mod write_head;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/:vault_id/head", get(head::handler))
        .route("/:vault_id/head", post(write_head::handler))
        .route("/:vault_id/log", get(log::handler))
        .with_state(state)
}
