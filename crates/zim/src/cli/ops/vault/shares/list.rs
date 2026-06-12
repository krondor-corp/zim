//! `zim vault <target> shares list` — show every shareholder.

use std::collections::HashMap;
use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::http_server::api::client::ApiError;
use crate::http_server::api::v0::peers::list::ListRequest as PeersListRequest;
use crate::http_server::api::v0::vault::shares::{ShareInfo, SharesRequest};

#[derive(Args, Debug, Clone)]
pub struct List {
    #[arg(skip)]
    pub target: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ListOutput {
    /// DID URL of the local peer — `shares[i].peer == you` flags the
    /// row that's "you" in the table render.
    pub you: String,
    pub shares: Vec<ShareInfo>,
    /// DID → nick from the local peer book, used to make the table
    /// readable. Empty if the book has no matching entry.
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
        let r = ctx.client.call(SharesRequest { vault_id }).await?;
        // Fetch the peer book so we can render nicks alongside hex.
        // The book is local-only (no network), so the cost is cheap.
        let peers = ctx.client.call(PeersListRequest {}).await?;
        let nicks: HashMap<String, String> =
            peers.peers.into_iter().map(|p| (p.did, p.nick)).collect();
        Ok(ListOutput {
            you: r.you,
            shares: r.shares,
            nicks,
        })
    }
}

impl fmt::Display for ListOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.shares.is_empty() {
            return writeln!(f, "{}", ui::dim("no shares"));
        }
        // Show nick when we have one, dim hex when we don't. Append
        // `(you)` on the local row.
        for s in &self.shares {
            let is_you = s.peer == self.you;
            let nick = self.nicks.get(&s.peer);
            match (nick, is_you) {
                (Some(n), true) => writeln!(f, "{} {} {}", n, ui::dim(&s.peer), ui::dim("(you)"))?,
                (Some(n), false) => writeln!(f, "{} {}", n, ui::dim(&s.peer))?,
                (None, true) => writeln!(f, "{} {}", s.peer, ui::dim("(you)"))?,
                (None, false) => writeln!(f, "{}", s.peer)?,
            }
        }
        Ok(())
    }
}
