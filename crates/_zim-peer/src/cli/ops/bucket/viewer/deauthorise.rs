use std::fmt;

use clap::Args;
use uuid::Uuid;

use crate::cli::ui;
use zim_peer::http_server::api::client::{resolve_bucket, ApiError};
use zim_peer::http_server::api::v0::bucket::viewer::{
    DeauthoriseViewerRequest, DeauthoriseViewerResponse,
};

#[derive(Args, Debug, Clone)]
pub struct Deauthorise {
    /// Bucket name or UUID
    pub bucket: String,
    /// Hex-encoded viewer pubkey to remove
    pub viewer_public_key: String,
}

#[derive(Debug)]
pub struct DeauthoriseOutput {
    pub bucket_id: Uuid,
    pub viewer_public_key: String,
    pub new_link: String,
}

impl fmt::Display for DeauthoriseOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{}",
            ui::success(
                "Deauthorised",
                &format!(
                    "viewer {} on bucket {}",
                    self.viewer_public_key, self.bucket_id
                )
            )
        )?;
        writeln!(f, "{}", ui::label("link", &self.new_link))?;
        // Surface the known gap to operators so they don't assume revocation
        // is cryptographic. T-001c-followup tracks the re-key work.
        write!(
            f,
            "  note: cached blobs the deauthorised viewer already fetched \
             remain decryptable until per-node secrets are rotated (T-001c-followup)."
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DeauthoriseError {
    #[error("API error: {0}")]
    Api(#[from] ApiError),
}

#[async_trait::async_trait]
impl crate::cli::op::Op for Deauthorise {
    type Error = DeauthoriseError;
    type Output = DeauthoriseOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        let mut client = ctx.client.clone();
        let bucket_id = resolve_bucket(&mut client, &self.bucket).await?;
        let response: DeauthoriseViewerResponse = client
            .call(DeauthoriseViewerRequest {
                bucket_id,
                viewer_public_key: self.viewer_public_key.clone(),
            })
            .await?;
        Ok(DeauthoriseOutput {
            bucket_id: response.bucket_id,
            viewer_public_key: response.viewer_public_key,
            new_link: response.new_bucket_link,
        })
    }
}
