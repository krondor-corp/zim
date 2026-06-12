//! `zim vault <target> relays <subcommand>` — relay-list ops.
//!
//! Same shape as [`super::shares`].

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;

pub mod add;
pub mod list;
pub mod rm;

pub use list::List;

/// Manage relays (list / add / rm). Bare `relays` lists.
#[derive(Args, Debug, Clone)]
pub struct Relays {
    #[command(subcommand)]
    pub command: Option<Command>,
    #[command(flatten)]
    pub list: List,
}

crate::command_enum! {
    /// Show every authorized relay on this vault.
    (List, list::List),
    /// Authorize a relay.
    (Add, add::Add),
    /// Revoke a relay.
    (Rm, rm::Rm),
}

#[async_trait]
impl Op for Relays {
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
