//! `zim vault <target> shares rm <peer>` — revoke a peer's share.

use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::daemon::api::client::ApiError;
use crate::daemon::api::v0::vault::unshare::UnshareRequest;

#[derive(Args, Debug, Clone)]
pub struct Rm {
    /// Vault id or name.
    pub target: String,
    /// Peer nick or hex pubkey.
    pub peer: String,
}

#[derive(Debug, serde::Serialize)]
pub struct RmOutput {
    pub peer: String,
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
        let peer = ctx.client.resolve_peer(&self.peer).await?;
        let r = ctx.client.call(UnshareRequest { vault_id, peer }).await?;
        Ok(RmOutput {
            peer: r.peer,
            height: r.height,
        })
    }
}

impl fmt::Display for RmOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} → height {}",
            ui::failure("unshared", &self.peer),
            ui::num(self.height.to_string())
        )
    }
}
