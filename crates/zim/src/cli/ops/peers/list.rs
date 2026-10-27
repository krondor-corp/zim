//! `zim peers list` — show all known nicknames.

use std::fmt;

use async_trait::async_trait;
use clap::Args;
use comfy_table::{Attribute, Cell};

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::daemon::api::client::ApiError;
use crate::daemon::api::v0::peers::list::{ListRequest, PeerInfo};

#[derive(Args, Debug, Clone)]
pub struct List;

#[derive(Debug, serde::Serialize)]
pub struct ListOutput(pub Vec<PeerInfo>);

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
        let r = ctx.client.call(ListRequest::default()).await?;
        // The hub stays in the book (it gates the hub's pushes) but it's
        // a relay, not something to share with — don't list it.
        Ok(ListOutput(
            r.peers.into_iter().filter(|p| !is_hub(&p.did)).collect(),
        ))
    }
}

/// A hub is a host-only `did:web:<host>`; a user is `did:web:<host>:u:<name>`.
fn is_hub(did: &str) -> bool {
    did.strip_prefix("did:web:")
        .is_some_and(|host| !host.contains(':'))
}

impl fmt::Display for ListOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            return writeln!(f, "{}", ui::dim("no peers — `zim peers add <nick> <did>`"));
        }
        let mut t = ui::make_table();
        t.set_header(vec![
            Cell::new("NICK").add_attribute(Attribute::Bold),
            Cell::new("DID").add_attribute(Attribute::Bold),
        ]);
        for p in &self.0 {
            t.add_row(vec![Cell::new(&p.nick), Cell::new(&p.did)]);
        }
        write!(f, "{t}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hub_is_hidden_but_alice_and_her_devices_are_listed() {
        assert!(is_hub("did:web:hub.example.com"));
        assert!(is_hub("did:web:localhost%3A17190"));

        assert!(!is_hub("did:web:hub.example.com:u:alice"));
        assert!(!is_hub(
            "did:key:z6MkiMxbZEgi5mgsMG24LT9sPaip9Ez699Nm83YqBNxAHuuV"
        ));
    }
}
