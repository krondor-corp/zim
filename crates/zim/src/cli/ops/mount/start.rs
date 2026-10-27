//! `zim mount start <vault|path>` — re-mount a stopped registration.

use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::daemon::api::client::ApiError;
use crate::daemon::api::v0::mounts::{MountInfo, StartRequest};

#[derive(Args, Debug, Clone)]
pub struct Start {
    /// The registration to mount — a vault name/id, or a mountpoint path.
    pub target: String,
}

#[derive(Debug, serde::Serialize)]
pub struct StartOutput {
    pub mounted: Vec<MountInfo>,
}

#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Start {
    type Context = ApiContext;
    type Output = StartOutput;
    type Error = StartError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let mountpoints = super::resolve_mountpoints(&ctx, &self.target).await?;
        let mut mounted = Vec::with_capacity(mountpoints.len());
        for mountpoint in mountpoints {
            let resp = ctx.client.call(StartRequest { mountpoint }).await?;
            mounted.push(resp.mount);
        }
        Ok(StartOutput { mounted })
    }
}

impl fmt::Display for StartOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, mount) in self.mounted.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{}", ui::success("mounted", &mount.mountpoint))?;
        }
        Ok(())
    }
}
