//! `zim mount <subcommand>` — mount a vault as a local directory via FUSE.
//!
//! These are thin HTTP clients over `/api/v0/mounts/*`; the actual mounting
//! happens in the daemon (built `--features fuse`). Against a daemon without
//! FUSE support the endpoints 404 and the command reports that.

use std::fmt;

use async_trait::async_trait;
use clap::Subcommand;

use crate::cli::op::Op;
use crate::context::ApiContext;
use crate::http_server::api::client::ApiError;
use crate::http_server::api::v0::mounts::ListRequest;

pub mod add;
pub mod list;
pub mod remove;
pub mod stop;

/// Resolve a `stop`/`remove` target — a mountpoint **path** or a **vault
/// name/id** — to the affected mountpoint(s). A path matches itself; a vault
/// matches *every* mount of that vault. An unknown target falls back to the
/// literal string, so operating on a stale/registered path still works.
pub(crate) async fn resolve_mountpoints(
    ctx: &ApiContext,
    target: &str,
) -> Result<Vec<String>, ApiError> {
    let mounts = ctx.client.call(ListRequest::default()).await?.mounts;
    // An exact mountpoint wins — an explicit path is unambiguous.
    if mounts.iter().any(|m| m.mountpoint == target) {
        return Ok(vec![target.to_string()]);
    }
    // Otherwise try it as a vault name/id and take that vault's mount(s).
    if let Ok(vault_id) = ctx.client.resolve_vault(target).await {
        let paths: Vec<String> = mounts
            .iter()
            .filter(|m| m.vault_id == vault_id)
            .map(|m| m.mountpoint.clone())
            .collect();
        if !paths.is_empty() {
            return Ok(paths);
        }
    }
    Ok(vec![target.to_string()])
}

#[derive(Subcommand, Debug, Clone)]
pub enum Mount {
    /// Mount a vault at a local path.
    Add(add::Add),
    /// List mounts and their status.
    List(list::List),
    /// Unmount, keeping the registration (re-`start` later / on boot).
    Stop(stop::Stop),
    /// Unmount and forget the registration.
    Remove(remove::Remove),
}

#[derive(Debug, serde::Serialize)]
#[serde(untagged)]
pub enum MountOutput {
    Add(add::AddOutput),
    List(list::ListOutput),
    Stop(stop::StopOutput),
    Remove(remove::RemoveOutput),
}

#[derive(Debug, thiserror::Error)]
pub enum MountError {
    #[error(transparent)]
    Add(#[from] add::AddError),
    #[error(transparent)]
    List(#[from] list::ListError),
    #[error(transparent)]
    Stop(#[from] stop::StopError),
    #[error(transparent)]
    Remove(#[from] remove::RemoveError),
}

#[async_trait]
impl Op for Mount {
    type Context = ();
    type Output = MountOutput;
    type Error = MountError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        Ok(match self {
            Mount::Add(c) => MountOutput::Add(c.run(c.build_context().await?).await?),
            Mount::List(c) => MountOutput::List(c.run(c.build_context().await?).await?),
            Mount::Stop(c) => MountOutput::Stop(c.run(c.build_context().await?).await?),
            Mount::Remove(c) => MountOutput::Remove(c.run(c.build_context().await?).await?),
        })
    }
}

impl fmt::Display for MountOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MountOutput::Add(o) => write!(f, "{o}"),
            MountOutput::List(o) => write!(f, "{o}"),
            MountOutput::Stop(o) => write!(f, "{o}"),
            MountOutput::Remove(o) => write!(f, "{o}"),
        }
    }
}
