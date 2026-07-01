//! `/api/v0` — version-namespaced endpoints.

use axum::Router;

pub mod mounts;
pub mod peers;
pub mod vault;
pub mod vaults;

use crate::ServiceState;

pub fn router(state: ServiceState) -> Router<ServiceState> {
    let router = Router::new()
        .nest("/peers", peers::router(state.clone()))
        .nest("/vaults", vaults::router(state.clone()))
        .nest("/vault/:vault_id", vault::router(state.clone()));
    // Mount endpoints exist only when the daemon is built with FUSE support.
    // Without it, still answer `/mounts/*` with a `501` (not a bare `404`) so
    // the condition is distinguishable — but keep the message neutral; build
    // flags are not the API's business to expose.
    #[cfg(feature = "fuse")]
    let router = router.nest("/mounts", mounts::router(state.clone()));
    #[cfg(not(feature = "fuse"))]
    let router = router.nest(
        "/mounts",
        Router::new().fallback(|| async {
            (
                axum::http::StatusCode::NOT_IMPLEMENTED,
                "filesystem mounting is not available on this daemon",
            )
        }),
    );
    router.with_state(state)
}
