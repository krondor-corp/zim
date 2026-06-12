use std::fmt;

use clap::Args;

use crate::cli::ui;
use zim_peer::http_server::api::client::ApiError;
use zim_peer::http_server::api::v0::bucket::sync::{ListSyncRequest, ListSyncResponse};

#[derive(Args, Debug, Clone)]
pub struct List {}

#[derive(Debug)]
pub struct ListSyncOutput {
    pub targets: Vec<zim_peer::http_server::api::v0::bucket::sync::SyncTargetInfo>,
}

impl fmt::Display for ListSyncOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.targets.is_empty() {
            return write!(f, "{}", ui::label("sync targets", &"(none)"));
        }
        writeln!(
            f,
            "{}",
            ui::label("sync targets", &self.targets.len().to_string())
        )?;
        for (i, t) in self.targets.iter().enumerate() {
            writeln!(f, "  {} → {} [{}]", t.bucket_id, t.target_path, t.status)?;
            if let Some(ref head) = t.last_head {
                write!(f, "    last_head: {}", &head[..head.len().min(16)])?;
            }
            if i + 1 < self.targets.len() {
                writeln!(f)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ListSyncError {
    #[error("API error: {0}")]
    Api(#[from] ApiError),
}

#[async_trait::async_trait]
impl crate::cli::op::Op for List {
    type Error = ListSyncError;
    type Output = ListSyncOutput;

    async fn execute(&self, ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        let mut client = ctx.client.clone();
        let response: ListSyncResponse = client.call(ListSyncRequest {}).await?;
        Ok(ListSyncOutput {
            targets: response.targets,
        })
    }
}
