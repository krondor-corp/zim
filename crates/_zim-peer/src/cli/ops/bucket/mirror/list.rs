use std::fmt;

use clap::Args;
use uuid::Uuid;

use crate::cli::ui;
use zim_peer::http_server::api::client::{resolve_bucket, ApiError};
use zim_peer::http_server::api::v0::bucket::mirror::{ListRelaysRequest, ListRelaysResponse};

#[derive(Args, Debug, Clone)]
pub struct List {
    /// Bucket name or UUID
    pub bucket: String,
}

#[derive(Debug)]
pub struct ListRelaysOutput {
    pub bucket_id: Uuid,
    pub relays: Vec<String>,
}

impl fmt::Display for ListRelaysOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}", ui::label("bucket", &self.bucket_id.to_string()))?;
        if self.relays.is_empty() {
            return write!(f, "{}", ui::label("relays", &"(none)"));
        }
        writeln!(f, "{}", ui::label("relays", &self.relays.len().to_string()))?;
        for (i, pk) in self.relays.iter().enumerate() {
            if i + 1 == self.relays.len() {
                write!(f, "  {}", pk)?;
            } else {
                writeln!(f, "  {}", pk)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ListRelaysError {
    #[error("API error: {0}")]
    Api(#[from] ApiError),
}

#[async_trait::async_trait]
impl crate::cli::op::Op for List {
    type Error = ListRelaysError;
    type Output = ListRelaysOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        let mut client = ctx.client.clone();
        let bucket_id = resolve_bucket(&mut client, &self.bucket).await?;
        let response: ListRelaysResponse = client.call(ListRelaysRequest { bucket_id }).await?;
        Ok(ListRelaysOutput {
            bucket_id: response.bucket_id,
            relays: response.relays,
        })
    }
}
