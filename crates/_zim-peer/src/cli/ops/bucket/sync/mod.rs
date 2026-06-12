use clap::{Args, Subcommand};

pub mod add;
pub mod list;
pub mod pause;
pub mod remove;
pub mod resume;

use crate::cli::op::Op;

crate::command_enum! {
    (Add, add::Add),
    (Remove, remove::Remove),
    (List, list::List),
    (Pause, pause::Pause),
    (Resume, resume::Resume),
}

pub type SyncCommand = Command;

#[derive(Args, Debug, Clone)]
pub struct Sync {
    #[command(subcommand)]
    pub command: SyncCommand,
}

#[async_trait::async_trait]
impl Op for Sync {
    type Error = OpError;
    type Output = OpOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        self.command.execute(ctx).await
    }
}
