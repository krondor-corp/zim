//! `zim mount add <vault> <path>` — mount a vault at a local directory.

use std::fmt;
use std::path::PathBuf;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::http_server::api::client::ApiError;
use crate::http_server::api::v0::mounts::{AddRequest, MountInfo};

#[derive(Args, Debug, Clone)]
pub struct Add {
    /// Vault id (hex) or name.
    pub vault: String,
    /// Local directory to mount at. Omit on macOS to default to
    /// `/Volumes/<vault>`, where Finder surfaces the volume.
    pub mountpoint: Option<PathBuf>,
    /// Re-mount automatically when the daemon starts.
    #[arg(long)]
    pub auto: bool,
    /// Mount read-only.
    #[arg(long = "read-only")]
    pub read_only: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct AddOutput(pub MountInfo);

#[derive(Debug, thiserror::Error)]
pub enum AddError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error("mountpoint is not valid UTF-8")]
    BadPath,
    #[error("a mountpoint is required (no default outside macOS)")]
    MountpointRequired,
}

/// Where to mount when the user didn't say. On macOS, `/Volumes/<vault>` —
/// FUSE volumes only surface in Finder under `/Volumes`. Elsewhere there's
/// no sensible default, so the caller must pass one.
fn default_mountpoint(vault: &str) -> Result<String, AddError> {
    #[cfg(target_os = "macos")]
    {
        // Last path segment, so a `did:`/hex/name with slashes can't escape.
        let name = vault.rsplit('/').next().unwrap_or(vault);
        Ok(format!("/Volumes/{name}"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = vault;
        Err(AddError::MountpointRequired)
    }
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
        let vault_id = ctx.client.resolve_vault(&self.vault).await?;
        let mountpoint = match &self.mountpoint {
            Some(p) => p.to_str().ok_or(AddError::BadPath)?.to_string(),
            None => default_mountpoint(&self.vault)?,
        };
        let resp = ctx
            .client
            .call(AddRequest {
                vault_id,
                mountpoint,
                auto_mount: self.auto,
                read_only: self.read_only,
            })
            .await?;
        Ok(AddOutput(resp.mount))
    }
}

impl fmt::Display for AddOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", ui::success("mounted", &self.0.mountpoint))?;
        write!(f, "  {}", ui::dim(self.0.vault_id.to_string()))
    }
}
