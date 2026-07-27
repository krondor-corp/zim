//! `zim mount set <vault|path> [--auto <bool>] [--read-only <bool>]` —
//! edit a registration in place (no remove + re-add dance). A
//! `--read-only` change on a live mount remounts it.

use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::daemon::api::client::ApiError;
use crate::daemon::api::v0::mounts::{MountInfo, SetRequest};

#[derive(Args, Debug, Clone)]
pub struct Set {
    /// The mount to edit — a vault name/id, or a mountpoint path.
    pub target: String,
    /// Re-mount automatically when the daemon starts.
    #[arg(long)]
    pub auto: Option<bool>,
    /// Mount read-only (remounts if currently mounted).
    #[arg(long = "read-only")]
    pub read_only: Option<bool>,
}

#[derive(Debug, serde::Serialize)]
pub struct SetOutput(pub Vec<MountInfo>);

#[derive(Debug, thiserror::Error)]
pub enum SetError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error("nothing to change — pass --auto and/or --read-only")]
    NoChanges,
}

#[async_trait]
impl Op for Set {
    type Context = ApiContext;
    type Output = SetOutput;
    type Error = SetError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        if self.auto.is_none() && self.read_only.is_none() {
            return Err(SetError::NoChanges);
        }
        let mountpoints = super::resolve_mountpoints(&ctx, &self.target).await?;
        let mut out = Vec::with_capacity(mountpoints.len());
        for mountpoint in mountpoints {
            let resp = ctx
                .client
                .call(SetRequest {
                    mountpoint,
                    auto_mount: self.auto,
                    read_only: self.read_only,
                })
                .await?;
            out.push(resp.mount);
        }
        Ok(SetOutput(out))
    }
}

impl fmt::Display for SetOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, m) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            let mut flags = Vec::new();
            if m.read_only {
                flags.push("ro");
            }
            if m.auto_mount {
                flags.push("auto");
            }
            write!(
                f,
                "{} {}",
                ui::success("updated", &m.mountpoint),
                ui::dim(if flags.is_empty() {
                    "(no flags)".to_string()
                } else {
                    flags.join(",")
                })
            )?;
        }
        Ok(())
    }
}
