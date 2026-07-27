use std::fmt;

use async_trait::async_trait;
use bytes::Bytes;
use clap::Args;

use crate::cli::op::Op;
use crate::context::{ApiContext, ContextError};
use crate::daemon::api::client::ApiError;
use crate::daemon::api::v0::vault::cat::CatRequest;

#[derive(Args, Debug, Clone)]
pub struct Cat {
    /// Vault id or name.
    pub target: String,
    pub path: String,
}

#[derive(Debug, serde::Serialize)]
pub struct CatOutput(pub Bytes);

#[derive(Debug, thiserror::Error)]
pub enum CatError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Cat {
    type Context = ApiContext;
    type Output = CatOutput;
    type Error = CatError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let vault_id = ctx.client.resolve_vault(&self.target).await?;
        let r = ctx
            .client
            .call(CatRequest {
                vault_id,
                path: super::normalize_path(&self.path),
            })
            .await?;
        Ok(CatOutput(r.bytes))
    }
}

impl fmt::Display for CatOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Lossy UTF-8 — binary callers should hit the HTTP API directly.
        write!(f, "{}", String::from_utf8_lossy(&self.0))
    }
}
