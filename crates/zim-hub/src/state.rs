use std::net::SocketAddr;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub listen_address: SocketAddr,
}

impl AppState {
    pub fn from_config(config: &Config) -> Self {
        Self {
            listen_address: config.listen_address,
        }
    }
}
