//! `/api/v0/vaults` — the vault registry and per-vault endpoints.
//!
//! One noun, two depths (mirroring the hub's route scheme):
//! `/api/v0/vaults/{create,list}` operate on the registry;
//! `/api/v0/vaults/:vault_id/<op>` operates on one vault, with
//! [`extractor::VaultHandle`] resolving the `:vault_id` segment.

use axum::routing::post;
use axum::Router;

pub mod add;
pub mod cat;
pub mod create;
pub mod extractor;
pub mod head;
pub mod list;
pub mod ls;
pub mod mkdir;
pub mod mv;
pub mod rm;
pub mod share;
pub mod shares;
pub mod sync;
pub mod unshare;

use crate::ServiceState;

pub fn router(state: ServiceState) -> Router<ServiceState> {
    Router::new()
        .route("/create", post(create::handler))
        .route("/list", post(list::handler))
        .nest("/:vault_id", per_vault(state.clone()))
        .with_state(state)
}

fn per_vault(state: ServiceState) -> Router<ServiceState> {
    Router::new()
        .route("/head", post(head::handler))
        .route("/ls", post(ls::handler))
        .route("/cat", post(cat::handler))
        .route("/add", post(add::handler))
        .route("/mkdir", post(mkdir::handler))
        .route("/rm", post(rm::handler))
        .route("/mv", post(mv::handler))
        .route("/share", post(share::handler))
        .route("/shares", post(shares::handler))
        .route("/unshare", post(unshare::handler))
        .route("/sync", post(sync::handler))
        .with_state(state)
}
