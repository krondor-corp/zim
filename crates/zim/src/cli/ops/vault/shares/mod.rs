//! `zim vault <target> shares <subcommand>` — share-list ops.
//!
//! Bare `shares` (no subcommand) defaults to `list` — same shape as
//! jig's `issues` group: `command: Option<Command>` plus a flattened
//! `List` so the no-subcommand path falls through to it.

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;

pub mod add;
pub mod list;
pub mod rm;

pub use list::List;

/// Manage shares (list / add / rm). Bare `shares` lists.
#[derive(Args, Debug, Clone)]
pub struct Shares {
    #[command(subcommand)]
    pub command: Option<Command>,
    #[command(flatten)]
    pub list: List,
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
        match &self.command {
            Some(cmd) => cmd.run(()).await,
            None => Command::List(self.list.clone()).run(()).await,
        }
    }
}
