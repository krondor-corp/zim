//! `/api/v0/vault/:vault_id` — per-vault endpoints.
//!
//! Every handler in this module takes a [`extractor::VaultHandle`]
//! that resolves the URL's `:vault_id` segment into an open vault.
//! Registry-level ops (list, create) live under
//! [`super::vaults`].

use axum::routing::post;
use axum::Router;

pub mod add;
pub mod cat;
pub mod extractor;
pub mod head;
pub mod ls;
pub mod mkdir;
pub mod mv;
pub mod relay;
pub mod relays;
pub mod rm;
pub mod share;
pub mod shares;
pub mod sync;
pub mod unrelay;
pub mod unshare;

use crate::ServiceState;

pub fn router(state: ServiceState) -> Router<ServiceState> {
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
        .route("/relay", post(relay::handler))
        .route("/relays", post(relays::handler))
        .route("/unrelay", post(unrelay::handler))
        .route("/sync", post(sync::handler))
        .with_state(state)
}
