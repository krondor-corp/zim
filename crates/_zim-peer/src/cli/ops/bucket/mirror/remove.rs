use std::fmt;

use clap::Args;
use uuid::Uuid;

use crate::cli::ui;
use zim_peer::http_server::api::client::{resolve_bucket, ApiError};
use zim_peer::http_server::api::v0::bucket::mirror::{RemoveRelayRequest, RemoveRelayResponse};

#[derive(Args, Debug, Clone)]
pub struct Remove {
    /// Bucket name or UUID
    pub bucket: String,
    /// Hex-encoded peer pubkey to remove
    pub peer_public_key: String,
}

#[derive(Debug)]
pub struct RemoveRelayOutput {
    pub bucket_id: Uuid,
    pub peer_public_key: String,
    pub removed: bool,
    pub new_link: String,
}

impl fmt::Display for RemoveRelayOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let verb = if self.removed {
            "Relay removed"
        } else {
            "Relay not present (no-op)"
        };
        writeln!(
            f,
            "{}",
            ui::success(
                verb,
                &format!("{} from bucket {}", self.peer_public_key, self.bucket_id)
            )
        )?;
        write!(f, "{}", ui::label("link", &self.new_link))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RemoveRelayError {
    #[error("API error: {0}")]
    Api(#[from] ApiError),
}

#[async_trait::async_trait]
impl crate::cli::op::Op for Remove {
    type Error = RemoveRelayError;
    type Output = RemoveRelayOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        let mut client = ctx.client.clone();
        let bucket_id = resolve_bucket(&mut client, &self.bucket).await?;
        let response: RemoveRelayResponse = client
            .call(RemoveRelayRequest {
                bucket_id,
                peer_public_key: self.peer_public_key.clone(),
            })
            .await?;
        Ok(RemoveRelayOutput {
            bucket_id: response.bucket_id,
            peer_public_key: response.peer_public_key,
            removed: response.removed,
            new_link: response.new_bucket_link,
        })
    }
}
