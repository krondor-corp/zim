//! `zim vault <target> relays add <peer>` — authorize a relay.

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
    /// Peer nick or hex pubkey.
    pub peer: String,
}

#[derive(Debug, serde::Serialize)]
pub struct AddOutput {
    pub peer: String,
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
        let peer = ctx.client.resolve_peer(&self.peer).await?;
        let r = ctx.client.call(RelayRequest { vault_id, peer }).await?;
        Ok(AddOutput {
            peer: r.peer,
            height: r.height,
        })
    }
}

impl fmt::Display for AddOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} → height {}",
            ui::success("relay added", &self.peer),
            ui::num(self.height.to_string())
        )
    }
}
