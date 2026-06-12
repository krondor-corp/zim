use std::fmt;
use std::path::PathBuf;

use clap::Args;

use crate::cli::ui;
use zim_peer::http_server::api::client::{resolve_bucket, ApiError};
use zim_peer::http_server::api::v0::bucket::folders_publish::{
    FoldersPublishRequest, FoldersPublishResponse,
};

#[derive(Args, Debug, Clone)]
pub struct Publish {
    /// Bucket name or UUID
    pub bucket: String,

    /// Path of the folder inside the bucket to publish
    pub path: PathBuf,

    /// Optional display path the gateway serves this under
    /// (defaults to `path` itself)
    #[arg(long)]
    pub display_path: Option<String>,
}

#[derive(Debug)]
pub struct PublishOutput {
    pub response: FoldersPublishResponse,
}

impl fmt::Display for PublishOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let r = &self.response;
        writeln!(f, "{}", ui::success("Published folder", &r.display_path))?;
        write!(
            f,
            "{}",
            ui::label("link", &ui::truncate(&r.new_bucket_link, 16))
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    #[error("API error: {0}")]
    Api(#[from] ApiError),
}

#[async_trait::async_trait]
impl crate::cli::op::Op for Publish {
    type Error = PublishError;
    type Output = PublishOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        let mut client = ctx.client.clone();
        let bucket_id = resolve_bucket(&mut client, &self.bucket).await?;
        let request = FoldersPublishRequest {
            bucket_id,
            path: self.path.clone(),
            display_path: self.display_path.clone(),
        };
        let response: FoldersPublishResponse = client.call(request).await?;
        Ok(PublishOutput { response })
    }
}
