pub mod admin;
pub mod auth;
pub mod blob;
pub mod devices;
pub mod escrow;
pub mod me;
pub mod vault;
pub mod vaults;

use axum::routing::get;
use axum::Router;

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/me", get(me::handler))
        // `/vaults` = the account's vault list; `/vaults/:id/…` = the
        // per-vault mirror surface (same `vault` module).
        .route("/vaults", get(vaults::handler))
        .nest("/blob", blob::router(state.clone()))
        .nest("/vaults", vault::router(state.clone()))
        .nest("/escrow", escrow::router(state.clone()))
        .nest("/devices", devices::router(state.clone()))
        .nest("/admin", admin::router(state.clone()))
        .nest("/auth", auth::router(state))
}
