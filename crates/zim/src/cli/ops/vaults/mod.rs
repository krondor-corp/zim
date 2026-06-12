//! `zim vaults <subcommand>` — registry-level ops: list every vault
//! the daemon knows, create a new one. Per-vault filesystem ops live
//! under `zim vault <target> <subcommand>` in the sibling [`vault`]
//! module.
//!
//! [`vault`]: super::vault

use std::fmt;

use async_trait::async_trait;
use clap::Subcommand;

use crate::cli::op::Op;

pub mod create;
pub mod list;

#[derive(Subcommand, Debug, Clone)]
pub enum Vaults {
    /// List every vault this peer knows.
    List(list::List),
    /// Create a new vault.
    Create(create::Create),
}

#[derive(Debug, serde::Serialize)]
#[serde(untagged)]
pub enum VaultsOutput {
    List(list::ListOutput),
    Create(create::CreateOutput),
}

#[derive(Debug, thiserror::Error)]
pub enum VaultsError {
    #[error(transparent)]
    List(#[from] list::ListError),
    #[error(transparent)]
    Create(#[from] create::CreateError),
}

#[async_trait]
impl Op for Vaults {
    type Context = ();
    type Output = VaultsOutput;
    type Error = VaultsError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        Ok(match self {
            Vaults::List(c) => VaultsOutput::List(c.run(c.build_context().await?).await?),
            Vaults::Create(c) => VaultsOutput::Create(c.run(c.build_context().await?).await?),
        })
    }
}

impl fmt::Display for VaultsOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VaultsOutput::List(o) => write!(f, "{o}"),
            VaultsOutput::Create(o) => write!(f, "{o}"),
        }
    }
}
