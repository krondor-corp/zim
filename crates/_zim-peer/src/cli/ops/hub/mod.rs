use clap::{Args, Subcommand};

pub mod login;
pub mod register;

use crate::cli::op::Op;

crate::command_enum! {
    (Register, register::Register),
    (Login, login::Login),
}

pub type HubCommand = Command;

#[derive(Args, Debug, Clone)]
pub struct Hub {
    #[command(subcommand)]
    pub command: HubCommand,
}

#[async_trait::async_trait]
impl Op for Hub {
    type Error = OpError;
    type Output = OpOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        self.command.execute(ctx).await
    }
}
