use std::env;
use std::net::SocketAddr;

use url::Url;

const DEFAULT_LISTEN: &str = "127.0.0.1:8080";
const DEFAULT_PEER: &str = "http://127.0.0.1:3001";
const LISTEN_ENV: &str = "ZIM_HUB_LISTEN";
const PEER_ENV: &str = "ZIM_HUB_PEER";
const LOG_ENV: &str = "ZIM_HUB_LOG";

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_address: SocketAddr,
    pub peer_endpoint: Url,
    pub log_level: tracing::Level,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid {LISTEN_ENV}: {0}")]
    InvalidListen(String),
    #[error("invalid {PEER_ENV}: {0}")]
    InvalidPeer(String),
    #[error("invalid {LOG_ENV}: {0}")]
    InvalidLogLevel(String),
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let listen_raw = env::var(LISTEN_ENV).unwrap_or_else(|_| DEFAULT_LISTEN.to_string());
        let listen_address = listen_raw
            .parse()
            .map_err(|_| ConfigError::InvalidListen(listen_raw.clone()))?;

        let peer_raw = env::var(PEER_ENV).unwrap_or_else(|_| DEFAULT_PEER.to_string());
        let mut peer_endpoint = peer_raw
            .parse()
            .map_err(|_| ConfigError::InvalidPeer(peer_raw.clone()))?;
        // Normalize: ensure trailing slash so url::Url::join keeps the full base path.
        if !peer_raw.ends_with('/') {
            peer_endpoint = format!("{peer_raw}/")
                .parse()
                .map_err(|_| ConfigError::InvalidPeer(peer_raw))?;
        }

        let log_level = match env::var(LOG_ENV) {
            Ok(s) => s
                .parse()
                .map_err(|_| ConfigError::InvalidLogLevel(s.clone()))?,
            Err(_) => tracing::Level::INFO,
        };

        Ok(Self {
            listen_address,
            peer_endpoint,
            log_level,
        })
    }
}
