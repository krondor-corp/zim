//! `zim vaults list` — show every vault known to the daemon.

use std::fmt;

use async_trait::async_trait;
use clap::Args;
use comfy_table::{Attribute, Cell};

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::http_server::api::client::ApiError;
use crate::http_server::api::v0::vaults::list::{ListRequest, VaultInfo};

#[derive(Args, Debug, Clone)]
pub struct List;

#[derive(Debug, serde::Serialize)]
pub struct ListOutput(pub Vec<VaultInfo>);

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
        let r = ctx.client.call(ListRequest {}).await?;
        Ok(ListOutput(r.vaults))
    }
}

impl fmt::Display for ListOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            return writeln!(f, "{}", ui::dim("no vaults — `zim vaults create <name>`"));
        }
        let mut t = ui::make_table();
        t.set_header(vec![
            Cell::new("NAME").add_attribute(Attribute::Bold),
            Cell::new("ID").add_attribute(Attribute::Bold),
        ]);
        for v in &self.0 {
            // Render the name when we know it; otherwise show the
            // open error inline so users can see *why* a vault is
            // broken rather than just a blank cell.
            let name_cell = match (&v.name, &v.error) {
                (Some(n), _) => n.clone(),
                (None, Some(err)) => format!("{} {}", ui::warning("broken", ""), ui::dim(err)),
                (None, None) => ui::dim("(no name)").to_string(),
            };
            t.add_row(vec![
                Cell::new(name_cell),
                Cell::new(v.vault_id.to_string()),
            ]);
        }
        write!(f, "{t}")
    }
}
