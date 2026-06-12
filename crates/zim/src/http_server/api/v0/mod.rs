//! `/api/v0` — version-namespaced endpoints.

use axum::Router;

pub mod peers;
pub mod vault;
pub mod vaults;

use crate::ServiceState;

pub fn router(state: ServiceState) -> Router<ServiceState> {
    Router::new()
        .nest("/peers", peers::router(state.clone()))
        .nest("/vaults", vaults::router(state.clone()))
        .nest("/vault/:vault_id", vault::router(state.clone()))
        .with_state(state)
}
