use std::fmt;
use std::io::Read;

use async_trait::async_trait;
use bytes::Bytes;
use clap::Args;
use zim_core::linked_data::Link;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::daemon::api::client::ApiError;
use crate::daemon::api::v0::vault::add::AddRequest;

#[derive(Args, Debug, Clone)]
pub struct Add {
    /// Vault id or name.
    pub target: String,
    pub path: String,
}

#[derive(Debug, serde::Serialize)]
pub struct AddOutput {
    pub path: String,
    pub link: Link,
    pub height: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum AddError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
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
        let mut buf = Vec::new();
        std::io::stdin().read_to_end(&mut buf)?;
        let r = ctx
            .client
            .call(AddRequest {
                vault_id,
                path: super::normalize_path(&self.path),
                bytes: Bytes::from(buf),
            })
            .await?;
        Ok(AddOutput {
            path: r.path,
            link: r.link,
            height: r.height,
        })
    }
}

impl fmt::Display for AddOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} → height {}",
            ui::success("added", &self.path),
            ui::num(self.height.to_string())
        )
    }
}
