//! `zim vault <op> …` — everything vault-shaped, verb-first like the
//! rest of the CLI (`mount add`, `peers add`). Registry ops
//! (`create`, `list`) and per-vault ops (`ls <target> /`, …) share the
//! one noun; per-vault leaves take the vault id/name as their first
//! positional.

use std::fmt;

use async_trait::async_trait;
use clap::Subcommand;

use crate::cli::op::Op;

pub mod add;
pub mod cat;
pub mod create;
pub mod head;
pub mod list;
pub mod ls;
pub mod mkdir;
pub mod mv;
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

#[derive(Subcommand, Debug, Clone)]
pub enum Vault {
    /// Create a new vault.
    Create(create::Create),
    /// List every vault this daemon holds.
    List(list::List),
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
    /// Pull from a peer.
    Sync(sync::Sync),
}

#[derive(Debug, serde::Serialize)]
#[serde(untagged)]
pub enum VaultOutput {
    Create(create::CreateOutput),
    List(list::ListOutput),
    Head(head::HeadOutput),
    Ls(ls::LsOutput),
    Cat(cat::CatOutput),
    Add(add::AddOutput),
    Mkdir(mkdir::MkdirOutput),
    Rm(rm::RmOutput),
    Mv(mv::MvOutput),
    Shares(shares::OpOutput),
    Sync(sync::SyncOutput),
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error(transparent)]
    Create(#[from] create::CreateError),
    #[error(transparent)]
    List(#[from] list::ListError),
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
        Ok(match self {
            Vault::Create(c) => VaultOutput::Create(c.run(c.build_context().await?).await?),
            Vault::List(c) => VaultOutput::List(c.run(c.build_context().await?).await?),
            Vault::Head(c) => VaultOutput::Head(c.run(c.build_context().await?).await?),
            Vault::Ls(c) => VaultOutput::Ls(c.run(c.build_context().await?).await?),
            Vault::Cat(c) => VaultOutput::Cat(c.run(c.build_context().await?).await?),
            Vault::Add(c) => VaultOutput::Add(c.run(c.build_context().await?).await?),
            Vault::Mkdir(c) => VaultOutput::Mkdir(c.run(c.build_context().await?).await?),
            Vault::Rm(c) => VaultOutput::Rm(c.run(c.build_context().await?).await?),
            Vault::Mv(c) => VaultOutput::Mv(c.run(c.build_context().await?).await?),
            Vault::Shares(c) => VaultOutput::Shares(c.run(c.build_context().await?).await?),
            Vault::Sync(c) => VaultOutput::Sync(c.run(c.build_context().await?).await?),
        })
    }
}

impl fmt::Display for VaultOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VaultOutput::Create(o) => write!(f, "{o}"),
            VaultOutput::List(o) => write!(f, "{o}"),
            VaultOutput::Head(o) => write!(f, "{o}"),
            VaultOutput::Ls(o) => write!(f, "{o}"),
            VaultOutput::Cat(o) => write!(f, "{o}"),
            VaultOutput::Add(o) => write!(f, "{o}"),
            VaultOutput::Mkdir(o) => write!(f, "{o}"),
            VaultOutput::Rm(o) => write!(f, "{o}"),
            VaultOutput::Mv(o) => write!(f, "{o}"),
            VaultOutput::Shares(o) => write!(f, "{o}"),
            VaultOutput::Sync(o) => write!(f, "{o}"),
        }
    }
}
