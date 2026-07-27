//! `zim vault create <name>` — create a new vault and print its id.

use std::fmt;

use async_trait::async_trait;
use clap::Args;
use zim_core::vault::VaultId;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::daemon::api::client::ApiError;
use crate::daemon::api::v0::vault::create::CreateRequest;

#[derive(Args, Debug, Clone)]
pub struct Create {
    pub name: String,
}

#[derive(Debug, serde::Serialize)]
pub struct CreateOutput {
    pub vault_id: VaultId,
    pub name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CreateError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Create {
    type Context = ApiContext;
    type Output = CreateOutput;
    type Error = CreateError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let r = ctx
            .client
            .call(CreateRequest {
                name: self.name.clone(),
            })
            .await?;
        Ok(CreateOutput {
            vault_id: r.vault_id,
            name: r.name,
        })
    }
}

impl fmt::Display for CreateOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({})",
            ui::success("created", &self.name),
            ui::dim(self.vault_id.to_string())
        )
    }
}
