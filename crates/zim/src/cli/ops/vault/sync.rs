use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::http_server::api::client::ApiError;
use crate::http_server::api::v0::vault::sync::SyncRequest;

#[derive(Args, Debug, Clone)]
pub struct Sync {
    #[arg(skip)]
    pub target: String,
    /// Hex-encoded peer to pull from.
    pub peer: String,
}

#[derive(Debug, serde::Serialize)]
pub struct SyncOutput {
    pub peer: String,
    pub height: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Sync {
    type Context = ApiContext;
    type Output = SyncOutput;
    type Error = SyncError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let vault_id = ctx.client.resolve_vault(&self.target).await?;
        let peer = ctx.client.resolve_peer(&self.peer).await?;
        let r = ctx.client.call(SyncRequest { vault_id, peer }).await?;
        Ok(SyncOutput {
            peer: r.peer,
            height: r.height,
        })
    }
}

impl fmt::Display for SyncOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} from {} → height {}",
            ui::success("synced", &self.peer),
            ui::ident(&self.peer),
            ui::num(self.height.to_string())
        )
    }
}
