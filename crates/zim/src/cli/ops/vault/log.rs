use std::fmt;

use async_trait::async_trait;
use clap::Args;
use zim_core::vault::VaultId;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::daemon::api::client::ApiError;
use crate::daemon::api::v0::vault::log::{LogEntry, LogRequest};

#[derive(Args, Debug, Clone)]
pub struct Log {
    /// Vault id or name.
    pub target: String,
    /// Start from this height instead of the head.
    #[arg(long)]
    pub from: Option<u64>,
    /// Max versions to show.
    #[arg(long)]
    pub limit: Option<u64>,
}

#[derive(Debug, serde::Serialize)]
pub struct LogOutput {
    pub vault_id: VaultId,
    pub entries: Vec<LogEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum LogError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Log {
    type Context = ApiContext;
    type Output = LogOutput;
    type Error = LogError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let vault_id = ctx.client.resolve_vault(&self.target).await?;
        let r = ctx
            .client
            .call(LogRequest {
                vault_id,
                from: self.from,
                limit: self.limit,
            })
            .await?;
        Ok(LogOutput {
            vault_id,
            entries: r.entries,
        })
    }
}

impl fmt::Display for LogOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.entries.is_empty() {
            return write!(f, "{}", ui::dim("no versions"));
        }
        for e in &self.entries {
            let author = e
                .author
                .as_deref()
                .map_or("unknown", |a| &a[..a.len().min(8)]);
            writeln!(
                f,
                "{} {} {}",
                ui::num(format!("@{}", e.height)),
                ui::dim(e.link.hash().to_string()),
                author,
            )?;
        }
        Ok(())
    }
}
