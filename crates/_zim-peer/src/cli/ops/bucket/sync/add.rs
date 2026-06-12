use std::fmt;

use clap::Args;
use uuid::Uuid;

use crate::cli::ui;
use zim_peer::http_server::api::client::{resolve_bucket, ApiError};
use zim_peer::http_server::api::v0::bucket::sync::{AddSyncRequest, AddSyncResponse};

#[derive(Args, Debug, Clone)]
pub struct Add {
    /// Bucket name or UUID
    pub bucket: String,
    /// Local filesystem path for the backup directory
    pub target_path: String,
}

#[derive(Debug)]
pub struct AddSyncOutput {
    pub id: Uuid,
    pub bucket_id: Uuid,
    pub target_path: String,
}

impl fmt::Display for AddSyncOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", ui::success("Sync target added", &self.target_path))?;
        writeln!(f, "{}", ui::label("id", &self.id.to_string()))?;
        write!(f, "{}", ui::label("bucket", &self.bucket_id.to_string()))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AddSyncError {
    #[error("API error: {0}")]
    Api(#[from] ApiError),
}

#[async_trait::async_trait]
impl crate::cli::op::Op for Add {
    type Error = AddSyncError;
    type Output = AddSyncOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        let mut client = ctx.client.clone();
        let bucket_id = resolve_bucket(&mut client, &self.bucket).await?;
        let response: AddSyncResponse = client
            .call(AddSyncRequest {
                bucket_id,
                target_path: self.target_path.clone(),
            })
            .await?;
        Ok(AddSyncOutput {
            id: response.id,
            bucket_id: response.bucket_id,
            target_path: response.target_path,
        })
    }
}
