use std::fmt;

use async_trait::async_trait;
use clap::Args;
use zim_core::linked_data::Link;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::http_server::api::client::ApiError;
use crate::http_server::api::v0::vault::mkdir::MkdirRequest;

#[derive(Args, Debug, Clone)]
pub struct Mkdir {
    #[arg(skip)]
    pub target: String,
    pub path: String,
    #[arg(long, short = 'p')]
    pub parents: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct MkdirOutput {
    pub path: String,
    pub link: Link,
    pub height: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum MkdirError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Mkdir {
    type Context = ApiContext;
    type Output = MkdirOutput;
    type Error = MkdirError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let vault_id = ctx.client.resolve_vault(&self.target).await?;
        let r = ctx
            .client
            .call(MkdirRequest {
                vault_id,
                path: super::normalize_path(&self.path),
                parents: self.parents,
            })
            .await?;
        Ok(MkdirOutput {
            path: r.path,
            link: r.link,
            height: r.height,
        })
    }
}

impl fmt::Display for MkdirOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} → height {}",
            ui::success("mkdir", &self.path),
            ui::num(self.height.to_string())
        )
    }
}
