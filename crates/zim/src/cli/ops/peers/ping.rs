//! `zim peers ping <nick-or-pubkey>` — connectivity + version probe.

use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::daemon::api::client::ApiError;
use crate::daemon::api::v0::peers::ping::PingRequest;

#[derive(Args, Debug, Clone)]
pub struct Ping {
    /// Peer to probe — a nickname from `zim peers list`, or a raw
    /// hex pubkey.
    pub peer: String,
}

#[derive(Debug, serde::Serialize)]
pub struct PingOutput {
    pub peer: String,
    pub peer_id_reported: String,
    pub version: String,
    pub uptime_secs: u64,
    pub rtt_ms: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum PingError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Ping {
    type Context = ApiContext;
    type Output = PingOutput;
    type Error = PingError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let pubkey = ctx.client.resolve_peer(&self.peer).await?;
        let r = ctx
            .client
            .call(PingRequest {
                peer: pubkey.clone(),
            })
            .await?;
        Ok(PingOutput {
            peer: self.peer.clone(),
            peer_id_reported: r.peer_id_reported,
            version: r.version,
            uptime_secs: r.uptime_secs,
            rtt_ms: r.rtt_ms,
        })
    }
}

impl fmt::Display for PingOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} {}  {}",
            ui::SUCCESS,
            ui::ident(&self.peer),
            ui::dim(&self.peer_id_reported)
        )?;
        writeln!(f, "rtt      {} ms", ui::num(self.rtt_ms.to_string()))?;
        writeln!(f, "uptime   {} s", ui::num(self.uptime_secs.to_string()))?;
        write!(f, "version  {}", ui::dim(&self.version))
    }
}
