use std::fmt;

use clap::Args;
use uuid::Uuid;

use crate::cli::ui;
use zim_peer::http_server::api::client::{resolve_bucket, ApiError};
use zim_peer::http_server::api::v0::bucket::mirror::{AddRelayRequest, AddRelayResponse};

#[derive(Args, Debug, Clone)]
pub struct Add {
    /// Bucket name or UUID
    pub bucket: String,
    /// Hex-encoded peer pubkey (Ed25519, 64 hex chars)
    pub peer_public_key: String,
}

#[derive(Debug)]
pub struct AddRelayOutput {
    pub bucket_id: Uuid,
    pub peer_public_key: String,
    pub new_link: String,
}

impl fmt::Display for AddRelayOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{}",
            ui::success(
                "Relay added",
                &format!("{} to bucket {}", self.peer_public_key, self.bucket_id)
            )
        )?;
        write!(f, "{}", ui::label("link", &self.new_link))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AddRelayError {
    #[error("API error: {0}")]
    Api(#[from] ApiError),
}

#[async_trait::async_trait]
impl crate::cli::op::Op for Add {
    type Error = AddRelayError;
    type Output = AddRelayOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        let mut client = ctx.client.clone();
        let bucket_id = resolve_bucket(&mut client, &self.bucket).await?;
        let response: AddRelayResponse = client
            .call(AddRelayRequest {
                bucket_id,
                peer_public_key: self.peer_public_key.clone(),
            })
            .await?;
        Ok(AddRelayOutput {
            bucket_id: response.bucket_id,
            peer_public_key: response.peer_public_key,
            new_link: response.new_bucket_link,
        })
    }
}
