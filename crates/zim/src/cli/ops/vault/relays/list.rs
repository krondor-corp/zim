//! `zim vault <target> relays list` — show every authorized relay.

use std::collections::HashMap;
use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::http_server::api::client::ApiError;
use crate::http_server::api::v0::peers::list::ListRequest as PeersListRequest;
use crate::http_server::api::v0::vault::relays::{RelayInfo, RelaysRequest};

#[derive(Args, Debug, Clone)]
pub struct List {
    #[arg(skip)]
    pub target: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ListOutput {
    pub relays: Vec<RelayInfo>,
    /// hex → nick from the local peer book.
    pub nicks: HashMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ListError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for List {
    type Context = ApiContext;
    type Output = ListOutput;
    type Error = ListError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let vault_id = ctx.client.resolve_vault(&self.target).await?;
        let r = ctx.client.call(RelaysRequest { vault_id }).await?;
        let peers = ctx.client.call(PeersListRequest {}).await?;
        let nicks: HashMap<String, String> =
            peers.peers.into_iter().map(|p| (p.did, p.nick)).collect();
        Ok(ListOutput {
            relays: r.relays,
            nicks,
        })
    }
}

impl fmt::Display for ListOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.relays.is_empty() {
            return writeln!(f, "{}", ui::dim("no relays"));
        }
        for r in &self.relays {
            let recipient_label = match self.nicks.get(&r.recipient) {
                Some(n) => format!("{} {}", n, ui::dim(&r.recipient)),
                None => r.recipient.clone(),
            };
            let via_label = match self.nicks.get(&r.via) {
                Some(n) => format!("{} {}", n, ui::dim(&r.via)),
                None => r.via.clone(),
            };
            writeln!(f, "{} via {}", recipient_label, via_label)?;
        }
        Ok(())
    }
}
