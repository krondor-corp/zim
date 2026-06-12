//! `zim id` — print this peer's pubkey (calls `/_status/identity`).

use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::http_server::api::client::ApiError;
use crate::http_server::health::identity::IdentityRequest;

#[derive(Args, Debug, Clone)]
pub struct Id;

#[derive(Debug, serde::Serialize)]
pub struct IdOutput(pub String);

#[derive(Debug, thiserror::Error)]
pub enum IdError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Id {
    type Context = ApiContext;
    type Output = IdOutput;
    type Error = IdError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let reply = ctx.client.call(IdentityRequest {}).await?;
        Ok(IdOutput(reply.node_id))
    }
}

impl fmt::Display for IdOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", ui::ident(&self.0))
    }
}
