use std::net::SocketAddr;

use crate::identity::IdentityStore;
use crate::peer_client::PeerClient;

/// Hub-side shared state. Holds:
/// - `peer`: in-process wrapper around the embedded zim-peer `ServiceState`.
/// - `identity`: zim-hub's own SQLite store for the encrypted-key vault (T-001a).
#[derive(Clone)]
pub struct AppState {
    pub listen_address: SocketAddr,
    pub peer: PeerClient,
    pub identity: IdentityStore,
}
