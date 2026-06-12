use clap::{Args, Subcommand};

pub mod authorize;
pub mod deauthorise;
pub mod list;

use crate::cli::op::Op;

crate::command_enum! {
    (List, list::List),
    (Authorize, authorize::Authorize),
    (Deauthorise, deauthorise::Deauthorise),
}

// Rename the generated Command to ViewerCommand for clarity
pub type ViewerCommand = Command;

#[derive(Args, Debug, Clone)]
pub struct Viewer {
    #[command(subcommand)]
    pub command: ViewerCommand,
}

#[async_trait::async_trait]
impl Op for Viewer {
    type Error = OpError;
    type Output = OpOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        self.command.execute(ctx).await
    }
}
