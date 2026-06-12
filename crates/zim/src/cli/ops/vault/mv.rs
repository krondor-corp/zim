use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::http_server::api::client::ApiError;
use crate::http_server::api::v0::vault::mv::MvRequest;

#[derive(Args, Debug, Clone)]
pub struct Mv {
    #[arg(skip)]
    pub target: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, serde::Serialize)]
pub struct MvOutput {
    pub from: String,
    pub to: String,
    pub height: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum MvError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Mv {
    type Context = ApiContext;
    type Output = MvOutput;
    type Error = MvError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let vault_id = ctx.client.resolve_vault(&self.target).await?;
        let r = ctx
            .client
            .call(MvRequest {
                vault_id,
                from: super::normalize_path(&self.from),
                to: super::normalize_path(&self.to),
            })
            .await?;
        Ok(MvOutput {
            from: r.from,
            to: r.to,
            height: r.height,
        })
    }
}

impl fmt::Display for MvOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} → {} (height {})",
            ui::success("moved", &self.from),
            ui::ident(&self.to),
            ui::num(self.height.to_string())
        )
    }
}
