pub mod auth;
pub mod devices;
pub mod escrow;
pub mod vault;

use axum::Router;

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .nest("/v", vault::router(state.clone()))
        .nest("/escrow", escrow::router(state.clone()))
        .nest("/devices", devices::router(state.clone()))
        .nest("/auth", auth::router(state))
}
