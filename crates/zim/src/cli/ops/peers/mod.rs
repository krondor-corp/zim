//! `zim peers <subcommand>` — local contacts CRUD + connectivity
//! probe. Backed by the `contacts` table in `log.sqlite` (see
//! `zim_peer::SqlitePeerStore`).

use std::fmt;

use async_trait::async_trait;
use clap::Subcommand;

use crate::cli::op::Op;

pub mod add;
pub mod list;
pub mod ping;
pub mod rm;

#[derive(Subcommand, Debug, Clone)]
pub enum Peers {
    /// List every nickname this daemon knows.
    List(list::List),
    /// Add (or replace) a nickname → pubkey mapping.
    Add(add::Add),
    /// Remove a nickname.
    Rm(rm::Rm),
    /// Round-trip ping over the existing sync protocol: identity,
    /// version, uptime, RTT.
    Ping(ping::Ping),
}

#[derive(Debug, serde::Serialize)]
#[serde(untagged)]
pub enum PeersOutput {
    List(list::ListOutput),
    Add(add::AddOutput),
    Rm(rm::RmOutput),
    Ping(ping::PingOutput),
}

#[derive(Debug, thiserror::Error)]
pub enum PeersError {
    #[error(transparent)]
    List(#[from] list::ListError),
    #[error(transparent)]
    Add(#[from] add::AddError),
    #[error(transparent)]
    Rm(#[from] rm::RmError),
    #[error(transparent)]
    Ping(#[from] ping::PingError),
}

#[async_trait]
impl Op for Peers {
    type Context = ();
    type Output = PeersOutput;
    type Error = PeersError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        Ok(match self {
            Peers::List(c) => PeersOutput::List(c.run(c.build_context().await?).await?),
            Peers::Add(c) => PeersOutput::Add(c.run(c.build_context().await?).await?),
            Peers::Rm(c) => PeersOutput::Rm(c.run(c.build_context().await?).await?),
            Peers::Ping(c) => PeersOutput::Ping(c.run(c.build_context().await?).await?),
        })
    }
}

impl fmt::Display for PeersOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PeersOutput::List(o) => write!(f, "{o}"),
            PeersOutput::Add(o) => write!(f, "{o}"),
            PeersOutput::Rm(o) => write!(f, "{o}"),
            PeersOutput::Ping(o) => write!(f, "{o}"),
        }
    }
}
