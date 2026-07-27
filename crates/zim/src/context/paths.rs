//! Path helpers — XDG-aware resolution for the zim data directory.
//!
//! Resolution priority (highest first):
//!
//! 1. `--config-path` CLI flag (explicit override, passed in)
//! 2. `$ZIM_HOME` env var
//! 3. `$XDG_CONFIG_HOME/zim` (XDG default)
//! 4. `~/.config/zim`
//!
//! **Debug builds** (`cfg!(debug_assertions)`, i.e. anything but
//! `--release`) nest the *default* location under a `debug/` subdir —
//! so a locally-installed debug binary (`./bin/install --dev`) keeps
//! its state isolated from a release install and is trivial to wipe
//! with `zim clean`. Explicit overrides (1, 2) are honoured verbatim:
//! if you name a path, that's the path.
//!
//! Layout under the resolved directory:
//!
//! ```text
//! $ZIM_HOME/
//! ├── config.toml      # AppConfig (api_port, log_level, …)
//! ├── identity.key     # Ed25519 secret, hex-encoded
//! ├── log.sqlite       # vault log + contacts book (SqliteVaultLog, SqlitePeerStore)
//! ├── blob-index.sqlite # SQLite index for the object-store blobs
//! ├── blobs/           # object-store blob bodies (BlobsProvider::local)
//! └── state/
//!     └── daemon.log   # daemon process log (future)
//! ```

use std::path::{Path, PathBuf};

/// Subdirectory appended to the *default* home in debug builds, so a
/// debug binary never shares state with a release install. Release
/// builds resolve to the bare default. See `with_profile_suffix`.
#[cfg(debug_assertions)]
pub const DEBUG_SUBDIR: &str = "debug";

/// Whether this binary resolves the default home under [`DEBUG_SUBDIR`].
/// True for any non-`--release` build. Surfaced so ops can label output
/// ("debug profile → ~/.config/zim/debug").
pub const fn is_debug_profile() -> bool {
    cfg!(debug_assertions)
}

/// Append the debug subdir to a *default*-resolved base in debug
/// builds; no-op in release. Explicit overrides never pass through here.
#[cfg(debug_assertions)]
fn with_profile_suffix(base: PathBuf) -> PathBuf {
    base.join(DEBUG_SUBDIR)
}
#[cfg(not(debug_assertions))]
fn with_profile_suffix(base: PathBuf) -> PathBuf {
    base
}

/// Base directory. Honours `$ZIM_HOME`, then `$XDG_CONFIG_HOME/zim`,
/// then `~/.config/zim`. The `cli_override` parameter wins outright
/// when present (used for the `--config-path` flag). Debug builds nest
/// the *default* path under [`DEBUG_SUBDIR`]; explicit overrides
/// (`cli_override`, `$ZIM_HOME`) are used verbatim.
pub fn home_dir(cli_override: Option<&Path>) -> Result<PathBuf, std::io::Error> {
    if let Some(p) = cli_override {
        return Ok(p.to_path_buf());
    }
    if let Some(v) = std::env::var_os("ZIM_HOME") {
        return Ok(PathBuf::from(v));
    }
    let base = if let Some(v) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(v).join("zim")
    } else {
        let home = dirs::home_dir().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "could not determine home directory",
            )
        })?;
        home.join(".config").join("zim")
    };
    Ok(with_profile_suffix(base))
}

pub fn config_file(home: &Path) -> PathBuf {
    home.join("config.toml")
}

pub fn identity_file(home: &Path) -> PathBuf {
    home.join("identity.key")
}

/// Where `zim hub login` persists the daemon's hub session — the URL we
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

/// SQLite index for the object-store blobs in [`blobs_dir`].
pub fn blob_index_file(home: &Path) -> PathBuf {
    home.join("blob-index.sqlite")
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
