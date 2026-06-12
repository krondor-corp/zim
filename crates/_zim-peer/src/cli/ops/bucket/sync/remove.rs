use std::fmt;

use clap::Args;

use crate::cli::ui;
use zim_peer::http_server::api::client::{resolve_bucket, ApiError};
use zim_peer::http_server::api::v0::bucket::sync::{RemoveSyncRequest, RemoveSyncResponse};

#[derive(Args, Debug, Clone)]
pub struct Remove {
    /// Bucket name or UUID
    pub bucket: String,
}

#[derive(Debug)]
pub struct RemoveSyncOutput {
    pub removed: bool,
}

impl fmt::Display for RemoveSyncOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.removed {
            write!(f, "{}", ui::success("Removed", "sync target"))
        } else {
            write!(f, "{}", ui::label("status", &"no sync target found"))
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RemoveSyncError {
    #[error("API error: {0}")]
    Api(#[from] ApiError),
}

#[async_trait::async_trait]
impl crate::cli::op::Op for Remove {
    type Error = RemoveSyncError;
    type Output = RemoveSyncOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        let mut client = ctx.client.clone();
        let bucket_id = resolve_bucket(&mut client, &self.bucket).await?;
        let response: RemoveSyncResponse = client.call(RemoveSyncRequest { bucket_id }).await?;
        Ok(RemoveSyncOutput {
            removed: response.removed,
        })
    }
}
