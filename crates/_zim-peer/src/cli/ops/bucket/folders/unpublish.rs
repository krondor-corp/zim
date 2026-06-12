use std::fmt;

use clap::Args;

use crate::cli::ui;
use zim_peer::http_server::api::client::{resolve_bucket, ApiError};
use zim_peer::http_server::api::v0::bucket::folders_unpublish::{
    FoldersUnpublishRequest, FoldersUnpublishResponse,
};

#[derive(Args, Debug, Clone)]
pub struct Unpublish {
    /// Bucket name or UUID
    pub bucket: String,

    /// Display path of the folder to unpublish
    pub display_path: String,
}

#[derive(Debug)]
pub struct UnpublishOutput {
    pub response: FoldersUnpublishResponse,
}

impl fmt::Display for UnpublishOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let r = &self.response;
        let verb = if r.removed {
            "Unpublished folder"
        } else {
            "No-op"
        };
        writeln!(f, "{}", ui::success(verb, &r.display_path))?;
        write!(
            f,
            "{}",
            ui::label("link", &ui::truncate(&r.new_bucket_link, 16))
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UnpublishError {
    #[error("API error: {0}")]
    Api(#[from] ApiError),
}

#[async_trait::async_trait]
impl crate::cli::op::Op for Unpublish {
    type Error = UnpublishError;
    type Output = UnpublishOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        let mut client = ctx.client.clone();
        let bucket_id = resolve_bucket(&mut client, &self.bucket).await?;
        let request = FoldersUnpublishRequest {
            bucket_id,
            display_path: self.display_path.clone(),
        };
        let response: FoldersUnpublishResponse = client.call(request).await?;
        Ok(UnpublishOutput { response })
    }
}
