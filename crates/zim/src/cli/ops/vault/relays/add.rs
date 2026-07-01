//! `zim vault <target> relays add <recipient> <via>` — authorize a relay.

use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::http_server::api::client::ApiError;
use crate::http_server::api::v0::vault::relay::RelayRequest;

#[derive(Args, Debug, Clone)]
pub struct Add {
    #[arg(skip)]
    pub target: String,
    /// Peer nick or DID for the ephemeral recipient (e.g. browser session key).
    pub recipient: String,
    /// Peer nick or DID for the always-on via peer (e.g. the hub).
    pub via: String,
}

#[derive(Debug, serde::Serialize)]
pub struct AddOutput {
    pub recipient: String,
    pub via: String,
    pub height: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum AddError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Add {
    type Context = ApiContext;
    type Output = AddOutput;
    type Error = AddError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let vault_id = ctx.client.resolve_vault(&self.target).await?;
        let recipient = ctx.client.resolve_peer(&self.recipient).await?;
        let via = ctx.client.resolve_peer(&self.via).await?;
        let r = ctx
            .client
            .call(RelayRequest {
                vault_id,
                recipient,
                via,
            })
            .await?;
        Ok(AddOutput {
            recipient: r.recipient,
            via: r.via,
            height: r.height,
        })
    }
}

impl fmt::Display for AddOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} via {} → height {}",
            ui::success("relay added", &self.recipient),
            ui::dim(&self.via),
            ui::num(self.height.to_string())
        )
    }
}
