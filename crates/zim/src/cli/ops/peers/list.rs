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
        Ok(ListOutput(r.peers))
    }
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
