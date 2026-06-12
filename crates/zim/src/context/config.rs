//! Persistent app config — `$ZIM_HOME/config.toml`.
//!
//! Loaded by every CLI command's `build_context()` and by `zim
//! daemon` at startup. Missing-file is fine; defaults are baked in.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::paths;
use super::ContextError;

/// Default daemon port. 17xxx range so it doesn't collide with the
/// old `_zim-peer`'s 5001 or anything common.
pub const DEFAULT_API_PORT: u16 = 17171;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Port for the daemon's HTTP API (loopback-only by default).
    pub api_port: u16,
    /// `tracing` log level used by the daemon's request tracing layer.
    pub log_level: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_port: DEFAULT_API_PORT,
            log_level: "info".to_string(),
        }
    }
}

impl AppConfig {
    /// Load from `<home>/config.toml`. Missing file → defaults.
    pub fn load(home: &Path) -> Result<Self, ContextError> {
        let path = paths::config_file(home);
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(&path)?;
        let cfg: Self = toml::from_str(&content)?;
        Ok(cfg)
    }

    /// Save to `<home>/config.toml`. Creates parent dirs as needed.
    pub fn save(&self, home: &Path) -> Result<(), ContextError> {
        let path = paths::config_file(home);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content =
            toml::to_string_pretty(self).map_err(|e| ContextError::Config(e.to_string()))?;
        fs::write(&path, content)?;
        Ok(())
    }
}
