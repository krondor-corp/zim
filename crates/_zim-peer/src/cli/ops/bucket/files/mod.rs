use clap::{Args, Subcommand};

pub mod publish;
pub mod unpublish;

use crate::cli::op::Op;

crate::command_enum! {
    (Publish, publish::Publish),
    (Unpublish, unpublish::Unpublish),
}

pub type FilesCommand = Command;

#[derive(Args, Debug, Clone)]
pub struct Files {
    #[command(subcommand)]
    pub command: FilesCommand,
}

#[async_trait::async_trait]
impl Op for Files {
    type Error = OpError;
    type Output = OpOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        self.command.execute(ctx).await
    }
}
