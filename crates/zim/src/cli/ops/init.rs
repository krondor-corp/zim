//! `zim init` — bootstrap the local peer state.
//!
//! Idempotent local setup, no daemon required:
//!
//! - resolve `$ZIM_HOME` (via `--config-path → $ZIM_HOME →
//!   $XDG_CONFIG_HOME/zim → ~/.config/zim`)
//! - create the data directory + `blobs/`, `state/` subdirs
//! - generate `identity.key` if missing (ed25519 secret, hex)
//! - write `config.toml` with defaults if missing
//!
//! After this, `zim daemon service install && zim daemon service start` will boot
//! a peer with a stable pubkey. Running `zim init` again on an
//! already-initialised home is a no-op (and reports the existing
//! identity).

use std::fmt;
use std::path::PathBuf;

use async_trait::async_trait;
use clap::Args;
use zim_crypto::PrivateKey;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{paths, AppConfig, ContextError};

#[derive(Args, Debug, Clone)]
pub struct Init;

#[derive(Debug, serde::Serialize)]
pub struct InitOutput {
    pub home: PathBuf,
    pub identity_hex: String,
    pub identity_was_new: bool,
    pub config_was_new: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("identity decode: {0}")]
    Identity(String),
}

#[async_trait]
impl Op for Init {
    type Context = ();
    type Output = InitOutput;
    type Error = InitError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        let home = paths::home_dir(None)?;
        paths::ensure_dirs(&home)?;

        let id_path = paths::identity_file(&home);
        let (secret, identity_was_new) = if id_path.exists() {
            let hex = tokio::fs::read_to_string(&id_path).await?;
            let secret =
                PrivateKey::from_hex(hex.trim()).map_err(|e| InitError::Identity(e.to_string()))?;
            (secret, false)
        } else {
            let secret = PrivateKey::generate();
            tokio::fs::write(&id_path, secret.to_hex()).await?;
            (secret, true)
        };

        let cfg_path = paths::config_file(&home);
        let config_was_new = !cfg_path.exists();
        if config_was_new {
            AppConfig::default().save(&home)?;
        }

        Ok(InitOutput {
            home,
            identity_hex: secret.public().to_hex(),
            identity_was_new,
            config_was_new,
        })
    }
}

impl fmt::Display for InitOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let id_tag = if self.identity_was_new {
            "new"
        } else {
            "existing"
        };
        let cfg_tag = if self.config_was_new {
            "new"
        } else {
            "existing"
        };
        writeln!(
            f,
            "{} {}",
            ui::success("initialized", ""),
            ui::dim(self.home.display().to_string())
        )?;
        writeln!(
            f,
            "  identity: {} ({})",
            ui::ident(&self.identity_hex),
            ui::dim(id_tag)
        )?;
        write!(
            f,
            "  config:   {} ({})",
            ui::dim(paths::config_file(&self.home).display().to_string()),
            ui::dim(cfg_tag)
        )
    }
}
