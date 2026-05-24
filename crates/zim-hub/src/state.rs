use std::net::SocketAddr;

use crate::config::Config;
use crate::peer_client::PeerClient;

#[derive(Clone)]
pub struct AppState {
    pub listen_address: SocketAddr,
    pub peer: PeerClient,
}

impl AppState {
    pub fn from_config(config: &Config) -> Self {
        Self {
            listen_address: config.listen_address,
            peer: PeerClient::new(config.peer_endpoint.clone()),
        }
    }
}
