//! HTTP server config — listen addr + tracing level.

use std::net::SocketAddr;

use url::Url;

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: SocketAddr,
    pub hostname: Url,
    pub log_level: tracing::Level,
}

impl Config {
    pub fn new(listen_addr: SocketAddr) -> Self {
        let hostname = Url::parse(&format!("http://localhost:{}", listen_addr.port()))
            .expect("hostname url must parse");
        Self {
            listen_addr,
            hostname,
            log_level: tracing::Level::INFO,
        }
    }

    pub fn with_log_level(mut self, level: tracing::Level) -> Self {
        self.log_level = level;
        self
    }
}
