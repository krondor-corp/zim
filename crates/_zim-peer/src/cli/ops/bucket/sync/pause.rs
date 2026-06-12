use std::fmt;

use clap::Args;

use crate::cli::ui;
use zim_peer::http_server::api::client::{resolve_bucket, ApiError};
use zim_peer::http_server::api::v0::bucket::sync::{PauseSyncRequest, StatusChangeResponse};

#[derive(Args, Debug, Clone)]
pub struct Pause {
    /// Bucket name or UUID
    pub bucket: String,
}

#[derive(Debug)]
pub struct PauseOutput {
    pub status: String,
}

impl fmt::Display for PauseOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", ui::success("Paused", &self.status))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PauseError {
    #[error("API error: {0}")]
    Api(#[from] ApiError),
}

#[async_trait::async_trait]
impl crate::cli::op::Op for Pause {
    type Error = PauseError;
    type Output = PauseOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        let mut client = ctx.client.clone();
        let bucket_id = resolve_bucket(&mut client, &self.bucket).await?;
        let response: StatusChangeResponse = client.call(PauseSyncRequest { bucket_id }).await?;
        Ok(PauseOutput {
            status: format!("{}", response.new_status),
        })
    }
}
