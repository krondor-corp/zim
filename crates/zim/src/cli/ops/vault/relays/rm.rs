//! `zim vault <target> relays rm <recipient>` — revoke a relay by recipient.

use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::http_server::api::client::ApiError;
use crate::http_server::api::v0::vault::unrelay::UnrelayRequest;

#[derive(Args, Debug, Clone)]
pub struct Rm {
    #[arg(skip)]
    pub target: String,
    /// Peer nick or DID of the recipient whose relay entry to remove.
    pub recipient: String,
}

#[derive(Debug, serde::Serialize)]
pub struct RmOutput {
    pub recipient: String,
    pub height: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum RmError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Rm {
    type Context = ApiContext;
    type Output = RmOutput;
    type Error = RmError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let vault_id = ctx.client.resolve_vault(&self.target).await?;
        let recipient = ctx.client.resolve_peer(&self.recipient).await?;
        let r = ctx
            .client
            .call(UnrelayRequest {
                vault_id,
                recipient,
            })
            .await?;
        Ok(RmOutput {
            recipient: r.recipient,
            height: r.height,
        })
    }
}

impl fmt::Display for RmOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} → height {}",
            ui::failure("relay removed", &self.recipient),
            ui::num(self.height.to_string())
        )
    }
}
