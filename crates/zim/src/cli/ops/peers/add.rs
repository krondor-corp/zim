//! `zim peers add <nick> <did>` — add or replace an entry.
//!
//! `<did>` is a DID URL (`did:key:z…` for daemons,
//! `did:web:hub.example.com:u:alice` for users). For convenience a
//! bare 64-char hex pubkey is also accepted and synthesised into a
//! `did:key:` URL client-side — saves users pasting raw hex.

use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::http_server::api::client::ApiError;
use crate::http_server::api::v0::peers::add::AddRequest;

#[derive(Args, Debug, Clone)]
pub struct Add {
    pub nick: String,
    /// DID URL of the peer (e.g. `did:key:z…`), or a 64-char hex
    /// pubkey (auto-promoted to `did:key:` for convenience).
    pub did: String,
    /// Optional free-form note attached to the entry.
    #[arg(long)]
    pub notes: Option<String>,
    /// Mark this contact trusted — auto-shared into the vaults you own.
    /// Off by default: untrusted contacts are shareable but opt-in per
    /// vault.
    #[arg(long)]
    pub trust: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct AddOutput {
    pub nick: String,
    pub did: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AddError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Add {
    type Context = ApiContext;
    type Output = AddOutput;
    type Error = AddError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        // resolve_peer handles the bare-hex → did:key promotion.
        let did = ctx.client.resolve_peer(&self.did).await?;
        let r = ctx
            .client
            .call(AddRequest {
                nick: self.nick.clone(),
                did,
                trusted: self.trust,
                notes: self.notes.clone(),
            })
            .await?;
        Ok(AddOutput {
            nick: r.nick,
            did: r.did,
        })
    }
}

impl fmt::Display for AddOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} → {}",
            ui::success("added", &self.nick),
            ui::dim(&self.did)
        )
    }
}
