//! `zim mount remove <vault|path>` — unmount and forget the registration.

use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::daemon::api::client::ApiError;
use crate::daemon::api::v0::mounts::RemoveRequest;

#[derive(Args, Debug, Clone)]
pub struct Remove {
    /// The mount to remove — a vault name/id, or a mountpoint path.
    pub target: String,
}

#[derive(Debug, serde::Serialize)]
pub struct RemoveOutput {
    /// `(mountpoint, removed)` for each affected mount.
    pub removed: Vec<(String, bool)>,
}

#[derive(Debug, thiserror::Error)]
pub enum RemoveError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Remove {
    type Context = ApiContext;
    type Output = RemoveOutput;
    type Error = RemoveError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let mountpoints = super::resolve_mountpoints(&ctx, &self.target).await?;
        let mut removed = Vec::with_capacity(mountpoints.len());
        for mountpoint in mountpoints {
            let resp = ctx
                .client
                .call(RemoveRequest {
                    mountpoint: mountpoint.clone(),
                })
                .await?;
            removed.push((mountpoint, resp.removed));
        }
        Ok(RemoveOutput { removed })
    }
}

impl fmt::Display for RemoveOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, (mountpoint, removed)) in self.removed.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            if *removed {
                write!(f, "{}", ui::success("removed", mountpoint))?;
            } else {
                write!(f, "{}", ui::dim(format!("no such mount: {mountpoint}")))?;
            }
        }
        Ok(())
    }
}
