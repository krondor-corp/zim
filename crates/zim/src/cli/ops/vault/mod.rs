//! `zim vault <target> <subcommand>` — per-vault filesystem and
//! membership ops. The positional `<target>` (UUID or human name)
//! lives on this wrapper; leaves get it propagated before dispatch.
//! Registry-level ops live under [`vaults`](super::vaults).

use std::fmt;

use async_trait::async_trait;
use clap::{Args, Subcommand};

use crate::cli::op::Op;

pub mod add;
pub mod cat;
pub mod head;
pub mod ls;
pub mod mkdir;
pub mod mv;
pub mod relays;
pub mod rm;
pub mod shares;
pub mod sync;

/// Normalise a user-supplied vault path to an absolute one before it
/// crosses the API boundary. The server is strict (`AbsPath::new`
/// rejects relative paths with a 400), but we want `cat foo.md` to do
/// the obviously-intended thing rather than spit a hex error at the
/// user.
///
/// Rules: empty / `.` → `/`; missing leading slash → prepend; otherwise
/// pass through. We deliberately don't try to resolve `..` or `.`
/// segments — the server already does path traversal correctly, and
/// our job here is just leading-slash convenience.
pub fn normalize_path(path: &str) -> String {
    if path.is_empty() || path == "." {
        return "/".to_string();
    }
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

#[derive(Args, Debug, Clone)]
pub struct Vault {
    /// Vault id or name.
    pub target: String,
    #[command(subcommand)]
    pub op: VaultSub,
}

#[derive(Subcommand, Debug, Clone)]
pub enum VaultSub {
    /// Show the current head + height.
    Head(head::Head),
    /// List directory contents.
    Ls(ls::Ls),
    /// Read a file to stdout.
    Cat(cat::Cat),
    /// Write a file from stdin into the vault.
    Add(add::Add),
    /// Create a directory.
    Mkdir(mkdir::Mkdir),
    /// Remove a path.
    Rm(rm::Rm),
    /// Move a path.
    Mv(mv::Mv),
    /// Manage shares (list / add / rm). Bare `shares` lists.
    Shares(shares::Shares),
    /// Manage relays (list / add / rm). Bare `relays` lists.
    Relays(relays::Relays),
    /// Pull from a peer.
    Sync(sync::Sync),
}

#[derive(Debug, serde::Serialize)]
#[serde(untagged)]
pub enum VaultOutput {
    Head(head::HeadOutput),
    Ls(ls::LsOutput),
    Cat(cat::CatOutput),
    Add(add::AddOutput),
    Mkdir(mkdir::MkdirOutput),
    Rm(rm::RmOutput),
    Mv(mv::MvOutput),
    Shares(shares::OpOutput),
    Relays(relays::OpOutput),
    Sync(sync::SyncOutput),
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error(transparent)]
    Head(#[from] head::HeadError),
    #[error(transparent)]
    Ls(#[from] ls::LsError),
    #[error(transparent)]
    Cat(#[from] cat::CatError),
    #[error(transparent)]
    Add(#[from] add::AddError),
    #[error(transparent)]
    Mkdir(#[from] mkdir::MkdirError),
    #[error(transparent)]
    Rm(#[from] rm::RmError),
    #[error(transparent)]
    Mv(#[from] mv::MvError),
    #[error(transparent)]
    Shares(#[from] shares::OpError),
    #[error(transparent)]
    Relays(#[from] relays::OpError),
    #[error(transparent)]
    Sync(#[from] sync::SyncError),
}

#[async_trait]
impl Op for Vault {
    type Context = ();
    type Output = VaultOutput;
    type Error = VaultError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        // Every leaf — and the Shares/Relays wrappers, which dispatch
        // further down — needs `target` to resolve the vault id. The
        // leaves' clap structs carry it as `#[arg(skip)]`; we stamp it
        // here before calling `run`. After that the pattern is the
        // same as `Peers` / `Vaults` / `Daemon`: match → run → wrap.
        let target = self.target.clone();
        Ok(match self.op.clone() {
            VaultSub::Head(mut c) => {
                c.target = target;
                VaultOutput::Head(c.run(c.build_context().await?).await?)
            }
            VaultSub::Ls(mut c) => {
                c.target = target;
                VaultOutput::Ls(c.run(c.build_context().await?).await?)
            }
            VaultSub::Cat(mut c) => {
                c.target = target;
                VaultOutput::Cat(c.run(c.build_context().await?).await?)
            }
            VaultSub::Add(mut c) => {
                c.target = target;
                VaultOutput::Add(c.run(c.build_context().await?).await?)
            }
            VaultSub::Mkdir(mut c) => {
                c.target = target;
                VaultOutput::Mkdir(c.run(c.build_context().await?).await?)
            }
            VaultSub::Rm(mut c) => {
                c.target = target;
                VaultOutput::Rm(c.run(c.build_context().await?).await?)
            }
            VaultSub::Mv(mut c) => {
                c.target = target;
                VaultOutput::Mv(c.run(c.build_context().await?).await?)
            }
            VaultSub::Sync(mut c) => {
                c.target = target;
                VaultOutput::Sync(c.run(c.build_context().await?).await?)
            }
            VaultSub::Shares(mut c) => {
                // Stamp `target` onto whichever leaf is about to
                // execute — explicit subcommand or the flattened
                // `list` default.
                match &mut c.command {
                    Some(shares::Command::List(l)) => l.target = target,
                    Some(shares::Command::Add(a)) => a.target = target,
                    Some(shares::Command::Rm(r)) => r.target = target,
                    None => c.list.target = target,
                }
                VaultOutput::Shares(c.run(c.build_context().await?).await?)
            }
            VaultSub::Relays(mut c) => {
                match &mut c.command {
                    Some(relays::Command::List(l)) => l.target = target,
                    Some(relays::Command::Add(a)) => a.target = target,
                    Some(relays::Command::Rm(r)) => r.target = target,
                    None => c.list.target = target,
                }
                VaultOutput::Relays(c.run(c.build_context().await?).await?)
            }
        })
    }
}

impl fmt::Display for VaultOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VaultOutput::Head(o) => write!(f, "{o}"),
            VaultOutput::Ls(o) => write!(f, "{o}"),
            VaultOutput::Cat(o) => write!(f, "{o}"),
            VaultOutput::Add(o) => write!(f, "{o}"),
            VaultOutput::Mkdir(o) => write!(f, "{o}"),
            VaultOutput::Rm(o) => write!(f, "{o}"),
            VaultOutput::Mv(o) => write!(f, "{o}"),
            VaultOutput::Shares(o) => write!(f, "{o}"),
            VaultOutput::Relays(o) => write!(f, "{o}"),
            VaultOutput::Sync(o) => write!(f, "{o}"),
        }
    }
}
