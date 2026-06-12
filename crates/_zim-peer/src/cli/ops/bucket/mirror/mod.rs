use clap::{Args, Subcommand};

pub mod add;
pub mod list;
pub mod remove;

use crate::cli::op::Op;

crate::command_enum! {
    (List, list::List),
    (Add, add::Add),
    (Remove, remove::Remove),
}

// Rename the generated Command to RelayCommand for clarity
pub type RelayCommand = Command;

#[derive(Args, Debug, Clone)]
pub struct Relay {
    #[command(subcommand)]
    pub command: RelayCommand,
}

#[async_trait::async_trait]
impl Op for Relay {
    type Error = OpError;
    type Output = OpOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        self.command.execute(ctx).await
    }
}
