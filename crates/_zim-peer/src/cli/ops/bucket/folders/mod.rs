use clap::{Args, Subcommand};

pub mod publish;
pub mod unpublish;

use crate::cli::op::Op;

crate::command_enum! {
    (Publish, publish::Publish),
    (Unpublish, unpublish::Unpublish),
}

pub type FoldersCommand = Command;

#[derive(Args, Debug, Clone)]
pub struct Folders {
    #[command(subcommand)]
    pub command: FoldersCommand,
}

#[async_trait::async_trait]
impl Op for Folders {
    type Error = OpError;
    type Output = OpOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        self.command.execute(ctx).await
    }
}
