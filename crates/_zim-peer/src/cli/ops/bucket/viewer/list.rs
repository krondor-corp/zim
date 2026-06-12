use std::fmt;

use clap::Args;
use uuid::Uuid;

use crate::cli::ui;
use zim_peer::http_server::api::client::{resolve_bucket, ApiError};
use zim_peer::http_server::api::v0::bucket::viewer::{ListViewersRequest, ListViewersResponse};

#[derive(Args, Debug, Clone)]
pub struct List {
    /// Bucket name or UUID
    pub bucket: String,
}

#[derive(Debug)]
pub struct ListViewersOutput {
    pub bucket_id: Uuid,
    pub viewers: Vec<String>,
}

impl fmt::Display for ListViewersOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", ui::label("bucket", &self.bucket_id.to_string()))?;
        if self.viewers.is_empty() {
            return write!(f, "{}", ui::label("viewers", &"(none)"));
        }
        writeln!(
            f,
            "{}",
            ui::label("viewers", &self.viewers.len().to_string())
        )?;
        for (i, pk) in self.viewers.iter().enumerate() {
            if i + 1 == self.viewers.len() {
                write!(f, "  {}", pk)?;
            } else {
                writeln!(f, "  {}", pk)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ListViewersError {
    #[error("API error: {0}")]
    Api(#[from] ApiError),
}

#[async_trait::async_trait]
impl crate::cli::op::Op for List {
    type Error = ListViewersError;
    type Output = ListViewersOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        let mut client = ctx.client.clone();
        let bucket_id = resolve_bucket(&mut client, &self.bucket).await?;
        let response: ListViewersResponse = client.call(ListViewersRequest { bucket_id }).await?;
        Ok(ListViewersOutput {
            bucket_id: response.bucket_id,
            viewers: response.viewers,
        })
    }
}
