//! Path helpers — XDG-aware resolution for the zim data directory.
//!
//! Resolution priority (highest first):
//!
//! 1. `--config-path` CLI flag (explicit override, passed in)
//! 2. `$ZIM_HOME` env var
//! 3. `$XDG_CONFIG_HOME/zim` (XDG default)
//! 4. `~/.config/zim`
//!
//! Layout under the resolved directory:
//!
//! ```text
//! $ZIM_HOME/
//! ├── config.toml      # AppConfig (api_port, log_level, …)
//! ├── identity.key     # Ed25519 secret, hex-encoded
//! ├── log.sqlite       # SqliteVaultLog (tracks every known vault)
//! ├── blobs/           # BlobsProvider::legacy_fs(...) store
//! └── state/
//!     └── daemon.log   # daemon process log (future)
//! ```

use std::path::{Path, PathBuf};

/// Base directory. Honours `$ZIM_HOME`, then `$XDG_CONFIG_HOME/zim`,
/// then `~/.config/zim`. The `cli_override` parameter wins outright
/// when present (used for the `--config-path` flag).
pub fn home_dir(cli_override: Option<&Path>) -> Result<PathBuf, std::io::Error> {
    if let Some(p) = cli_override {
        return Ok(p.to_path_buf());
    }
    if let Some(v) = std::env::var_os("ZIM_HOME") {
        return Ok(PathBuf::from(v));
    }
    if let Some(v) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(v).join("zim"));
    }
    let home = dirs::home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "could not determine home directory",
        )
    })?;
    Ok(home.join(".config").join("zim"))
}

pub fn config_file(home: &Path) -> PathBuf {
    home.join("config.toml")
}

/// Local peer address book — nickname → pubkey, owned by the daemon.
/// See `crate::peers`.
pub fn peers_file(home: &Path) -> PathBuf {
    home.join("peers.toml")
}

pub fn identity_file(home: &Path) -> PathBuf {
    home.join("identity.key")
}

/// Where `zim login` persists the daemon's hub session — the URL we
/// authenticated against + the bearer token from the device-code
/// poll. Read by the daemon on startup so it can talk to the hub
/// without re-prompting.
pub fn hub_session_file(home: &Path) -> PathBuf {
    home.join("hub-session.json")
}

pub fn log_file(home: &Path) -> PathBuf {
    home.join("log.sqlite")
}

pub fn blobs_dir(home: &Path) -> PathBuf {
    home.join("blobs")
}

pub fn state_dir(home: &Path) -> PathBuf {
    home.join("state")
}

pub fn daemon_log_path(home: &Path) -> PathBuf {
    state_dir(home).join("daemon.log")
}

/// Create the directories that `Vault::init` + the daemon need at
/// startup. Idempotent.
pub fn ensure_dirs(home: &Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(home)?;
    std::fs::create_dir_all(blobs_dir(home))?;
    std::fs::create_dir_all(state_dir(home))?;
    Ok(())
}
