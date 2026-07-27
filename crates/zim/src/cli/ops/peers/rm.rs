//! `zim peers rm <nick>` — drop an address-book entry.

use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::daemon::api::client::ApiError;
use crate::daemon::api::v0::peers::rm::RmRequest;

#[derive(Args, Debug, Clone)]
pub struct Rm {
    pub nick: String,
}

#[derive(Debug, serde::Serialize)]
pub struct RmOutput {
    pub nick: String,
    pub did: String,
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
        let r = ctx
            .client
            .call(RmRequest {
                nick: self.nick.clone(),
            })
            .await?;
        Ok(RmOutput {
            nick: r.nick,
            did: r.did,
        })
    }
}

impl fmt::Display for RmOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({})",
            ui::failure("removed", &self.nick),
            ui::dim(&self.did)
        )
    }
}
