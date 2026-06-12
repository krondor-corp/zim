//! `/api/v0/vaults` — registry-level endpoints. List the set of
//! vaults this peer knows; create a new vault. Per-vault operations
//! live under `/api/v0/vault/:vault_id/...` in the sibling
//! [`super::vault`] module.

use axum::routing::post;
use axum::Router;

pub mod create;
pub mod list;

use crate::ServiceState;

pub fn router(state: ServiceState) -> Router<ServiceState> {
    Router::new()
        .route("/list", post(list::handler))
        .route("/create", post(create::handler))
        .with_state(state)
}
