//! `zim vault shares <subcommand> <target> …` — share-list ops.

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;

pub mod add;
pub mod list;
pub mod rm;

pub use list::List;

/// Manage shares (list / add / rm).
#[derive(Args, Debug, Clone)]
pub struct Shares {
    #[command(subcommand)]
    pub command: Command,
}

crate::command_enum! {
    /// Show every shareholder on this vault.
    (List, list::List),
    /// Grant a peer access.
    (Add, add::Add),
    /// Revoke a peer's share.
    (Rm, rm::Rm),
}

#[async_trait]
impl Op for Shares {
    type Context = ();
    type Output = OpOutput;
    type Error = OpError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        self.command.run(()).await
    }
}
