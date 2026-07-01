//! `zim mount stop <vault|path>` — unmount, keeping the registration.

use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::http_server::api::client::ApiError;
use crate::http_server::api::v0::mounts::StopRequest;

#[derive(Args, Debug, Clone)]
pub struct Stop {
    /// The mount to unmount — a vault name/id, or a mountpoint path.
    pub target: String,
}

#[derive(Debug, serde::Serialize)]
pub struct StopOutput {
    pub unmounted: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum StopError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Stop {
    type Context = ApiContext;
    type Output = StopOutput;
    type Error = StopError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let mountpoints = super::resolve_mountpoints(&ctx, &self.target).await?;
        let mut unmounted = Vec::with_capacity(mountpoints.len());
        for mountpoint in mountpoints {
            ctx.client
                .call(StopRequest {
                    mountpoint: mountpoint.clone(),
                })
                .await?;
            unmounted.push(mountpoint);
        }
        Ok(StopOutput { unmounted })
    }
}

impl fmt::Display for StopOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, mountpoint) in self.unmounted.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{}", ui::success("unmounted", mountpoint))?;
        }
        Ok(())
    }
}
