//! Hub-side shared state. Held by every axum handler via `State`.
//!
//! Holds the embedded zim `Peer` + the persistent `Database` (users,
//! peer registrations, escrow). Both are clone-cheap so cloning
//! `AppState` is just bumping a couple of Arcs.

use std::net::SocketAddr;

use crate::config::{AuthConfig, Config};
use crate::database::Database;

#[derive(Clone)]
pub struct AppState {
    pub listen_address: SocketAddr,
    /// The hub's `did:web:<host>` identity URL.
    pub did: String,
    /// Public hostname the hub answers as (host part of `did`).
    pub host: String,
    /// In-process peer (ciphertext mirror + sync).
    pub service: zim::ServiceState,
    /// All persistent hub state — users, peer registrations,
    /// escrowed keys. Models are reached via
    /// `state.db.<Model>::<op>(...)`.
    pub db: Database,
    /// OAuth + cookie config.
    pub auth: AuthConfig,
}

impl AppState {
    pub fn new(config: &Config, service: zim::ServiceState, db: Database) -> Self {
        Self {
            listen_address: config.listen_address,
            did: config.did(),
            host: config.host.clone(),
            service,
            db,
            auth: config.auth.clone(),
        }
    }
}
