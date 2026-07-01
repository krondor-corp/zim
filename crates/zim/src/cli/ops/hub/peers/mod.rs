//! `zim hub peers <subcommand>` — manage the local address book from
//! the hub's device roster.
//!
//! These are a convenience layer over the base `zim peers` commands:
//! instead of pasting each device's DID by hand, they read the roster
//! the hub already tracks for your account and reconcile it into the
//! local book. They require a hub session (`zim hub login`); the base
//! `zim peers` commands never do.

use std::fmt;

use async_trait::async_trait;
use clap::Subcommand;

use crate::cli::op::Op;

pub mod list;
pub mod sync;

#[derive(Subcommand, Debug, Clone)]
pub enum HubPeers {
    /// Pull every device your hub account knows into the local address
    /// book (idempotent).
    Sync(sync::Sync),
    /// List the hub's device roster, marking which are already in your
    /// local address book.
    #[command(visible_alias = "ls")]
    List(list::List),
}

#[derive(Debug, serde::Serialize)]
#[serde(untagged)]
pub enum HubPeersOutput {
    Sync(sync::SyncOutput),
    List(list::ListOutput),
}

#[derive(Debug, thiserror::Error)]
pub enum HubPeersError {
    #[error(transparent)]
    Sync(#[from] sync::SyncError),
    #[error(transparent)]
    List(#[from] list::ListError),
}

#[async_trait]
impl Op for HubPeers {
    type Context = ();
    type Output = HubPeersOutput;
    type Error = HubPeersError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        Ok(match self {
            HubPeers::Sync(c) => HubPeersOutput::Sync(c.run(c.build_context().await?).await?),
            HubPeers::List(c) => HubPeersOutput::List(c.run(c.build_context().await?).await?),
        })
    }
}

impl fmt::Display for HubPeersOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HubPeersOutput::Sync(o) => write!(f, "{o}"),
            HubPeersOutput::List(o) => write!(f, "{o}"),
        }
    }
}
