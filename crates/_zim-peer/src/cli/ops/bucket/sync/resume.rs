use std::fmt;

use clap::Args;

use crate::cli::ui;
use zim_peer::http_server::api::client::{resolve_bucket, ApiError};
use zim_peer::http_server::api::v0::bucket::sync::{PauseSyncRequest, StatusChangeResponse};

#[derive(Args, Debug, Clone)]
pub struct Resume {
    /// Bucket name or UUID
    pub bucket: String,
}

#[derive(Debug)]
pub struct ResumeOutput {
    pub status: String,
}

impl fmt::Display for ResumeOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", ui::success("Resumed", &self.status))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResumeError {
    #[error("API error: {0}")]
    Api(#[from] ApiError),
}

#[async_trait::async_trait]
impl crate::cli::op::Op for Resume {
    type Error = ResumeError;
    type Output = ResumeOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        let mut client = ctx.client.clone();
        let bucket_id = resolve_bucket(&mut client, &self.bucket).await?;
        let response: StatusChangeResponse = client.call(PauseSyncRequest { bucket_id }).await?;
        Ok(ResumeOutput {
            status: format!("{}", response.new_status),
        })
    }
}
