//! Daemon HTTP server config — bind addr + tracing level.
//!
//! Built from `AppConfig` at daemon startup. Kept separate so the
//! `http_server` module doesn't pull in `AppConfig` directly.

use std::net::SocketAddr;

use url::Url;

#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub listen_addr: SocketAddr,
    pub hostname: Url,
    pub log_level: tracing::Level,
}

impl ServiceConfig {
    pub fn new(listen_addr: SocketAddr, log_level: tracing::Level) -> Self {
        let hostname = Url::parse(&format!("http://localhost:{}", listen_addr.port()))
            .expect("hostname url must parse");
        Self {
            listen_addr,
            hostname,
            log_level,
        }
    }
}
