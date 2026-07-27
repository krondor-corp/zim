use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::daemon::api::client::ApiError;
use crate::daemon::api::v0::vault::rm::RmRequest;

#[derive(Args, Debug, Clone)]
pub struct Rm {
    /// Vault id or name.
    pub target: String,
    pub path: String,
}

#[derive(Debug, serde::Serialize)]
pub struct RmOutput {
    pub path: String,
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
        let r = ctx
            .client
            .call(RmRequest {
                vault_id,
                path: super::normalize_path(&self.path),
            })
            .await?;
        Ok(RmOutput {
            path: r.path,
            height: r.height,
        })
    }
}

impl fmt::Display for RmOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} → height {}",
            ui::failure("removed", &self.path),
            ui::num(self.height.to_string())
        )
    }
}
