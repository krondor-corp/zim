use std::env;
use std::net::SocketAddr;

const DEFAULT_LISTEN: &str = "127.0.0.1:8080";
const LISTEN_ENV: &str = "ZIM_HUB_LISTEN";
const LOG_ENV: &str = "ZIM_HUB_LOG";

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_address: SocketAddr,
    pub log_level: tracing::Level,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid {LISTEN_ENV}: {0}")]
    InvalidListen(String),
    #[error("invalid {LOG_ENV}: {0}")]
    InvalidLogLevel(String),
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let listen_raw = env::var(LISTEN_ENV).unwrap_or_else(|_| DEFAULT_LISTEN.to_string());
        let listen_address = listen_raw
            .parse()
            .map_err(|_| ConfigError::InvalidListen(listen_raw.clone()))?;

        let log_level = match env::var(LOG_ENV) {
            Ok(s) => s
                .parse()
                .map_err(|_| ConfigError::InvalidLogLevel(s.clone()))?,
            Err(_) => tracing::Level::INFO,
        };

        Ok(Self {
            listen_address,
            log_level,
        })
    }
}
