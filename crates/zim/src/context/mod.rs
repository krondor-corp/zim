//! Context module — paths, configuration, and the typed contexts
//! handed to each CLI command's `Op::run`.
//!
//! Per-command context types:
//!
//! - `ApiContext` — most ops: holds an `ApiClient` pointed at the
//!   local daemon, the resolved `home` dir, and the loaded
//!   `AppConfig`. Built by reading `--config-path → $ZIM_HOME →
//!   $XDG_CONFIG_HOME/zim → ~/.config/zim`, then constructing a
//!   reqwest client at `http://127.0.0.1:<api_port>`.
//! - `DaemonContext` — `zim daemon`: same inputs but doesn't open an
//!   HTTP client; instead the daemon binds the listen address itself.

use std::path::{Path, PathBuf};

use url::Url;

pub mod config;
pub mod paths;

pub use config::AppConfig;

use crate::http_server::api::client::{ApiClient, ApiError};

#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("toml parse: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("api client: {0}")]
    Api(#[from] ApiError),
    #[error("url: {0}")]
    Url(#[from] url::ParseError),
    #[error("config: {0}")]
    Config(String),
}

/// Context for any command that talks to a running daemon.
pub struct ApiContext {
    pub home: PathBuf,
    pub config: AppConfig,
    pub client: ApiClient,
}

impl ApiContext {
    /// Build by resolving the home dir from `cli_override → $ZIM_HOME
    /// → $XDG_CONFIG_HOME/zim → ~/.config/zim`, loading the config
    /// (defaults if missing), and constructing the API client.
    pub fn build(cli_override: Option<&Path>) -> Result<Self, ContextError> {
        let home = paths::home_dir(cli_override)?;
        let config = AppConfig::load(&home)?;
        let endpoint = Url::parse(&format!("http://127.0.0.1:{}", config.api_port))?;
        let client = ApiClient::new(&endpoint)?;
        Ok(Self {
            home,
            config,
            client,
        })
    }
}

/// Context for `zim daemon` — same resolved paths, no HTTP client
/// (the daemon binds the listener itself).
pub struct DaemonContext {
    pub home: PathBuf,
    pub config: AppConfig,
}

impl DaemonContext {
    pub fn build(cli_override: Option<&Path>) -> Result<Self, ContextError> {
        let home = paths::home_dir(cli_override)?;
        paths::ensure_dirs(&home)?;
        let config = AppConfig::load(&home)?;
        Ok(Self { home, config })
    }
}
