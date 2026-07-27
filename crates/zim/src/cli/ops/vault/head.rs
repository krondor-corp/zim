use std::fmt;

use async_trait::async_trait;
use clap::Args;
use zim_core::linked_data::Link;
use zim_core::vault::VaultId;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::daemon::api::client::ApiError;
use crate::daemon::api::v0::vault::head::HeadRequest;

#[derive(Args, Debug, Clone)]
pub struct Head {
    /// Vault id or name.
    pub target: String,
}

#[derive(Debug, serde::Serialize)]
pub struct HeadOutput {
    pub vault_id: VaultId,
    pub link: Link,
    pub height: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum HeadError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Head {
    type Context = ApiContext;
    type Output = HeadOutput;
    type Error = HeadError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let vault_id = ctx.client.resolve_vault(&self.target).await?;
        let r = ctx.client.call(HeadRequest { vault_id }).await?;
        Ok(HeadOutput {
            vault_id: r.vault_id,
            link: r.link,
            height: r.height,
        })
    }
}

impl fmt::Display for HeadOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "vault {} @ height {} ({})",
            ui::ident(self.vault_id.to_string()),
            ui::num(self.height.to_string()),
            ui::dim(self.link.hash().to_string()),
        )
    }
}
