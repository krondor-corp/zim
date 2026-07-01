//! `/api/v0/peers` — local address book CRUD + ping.
//!
//! Per-daemon. Not shared in any vault manifest. Backs the
//! `zim peers <subcommand>` CLI and the resolver hook every place a
//! pubkey can be substituted for a nickname (`share`, `unshare`,
//! `relay`, `unrelay`, `sync`).

use axum::routing::post;
use axum::Router;

pub mod add;
pub mod list;
pub mod ping;
pub mod reconcile;
pub mod rm;

use crate::ServiceState;

pub fn router(state: ServiceState) -> Router<ServiceState> {
    Router::new()
        .route("/list", post(list::handler))
        .route("/add", post(add::handler))
        .route("/rm", post(rm::handler))
        .route("/ping", post(ping::handler))
        .route("/reconcile", post(reconcile::handler))
        .with_state(state)
}
