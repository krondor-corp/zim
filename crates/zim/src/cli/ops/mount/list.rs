//! `zim mount list` — show mounts and their status.

use std::fmt;

use async_trait::async_trait;
use clap::Args;
use comfy_table::{Attribute, Cell};

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::http_server::api::client::ApiError;
use crate::http_server::api::v0::mounts::{ListRequest, MountInfo};

#[derive(Args, Debug, Clone)]
pub struct List;

#[derive(Debug, serde::Serialize)]
pub struct ListOutput(pub Vec<MountInfo>);

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
        Ok(ListOutput(
            ctx.client.call(ListRequest::default()).await?.mounts,
        ))
    }
}

impl fmt::Display for ListOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            return writeln!(
                f,
                "{}",
                ui::dim("no mounts — `zim mount add <vault> <path>`")
            );
        }
        let mut t = ui::make_table();
        t.set_header(vec![
            Cell::new("NAME").add_attribute(Attribute::Bold),
            Cell::new("PATH").add_attribute(Attribute::Bold),
            Cell::new("VAULT").add_attribute(Attribute::Bold),
            Cell::new("STATE").add_attribute(Attribute::Bold),
            Cell::new("FLAGS").add_attribute(Attribute::Bold),
        ]);
        for m in &self.0 {
            let state = if m.mounted { "mounted" } else { "stopped" };
            let mut flags = Vec::new();
            if m.read_only {
                flags.push("ro");
            }
            if m.auto_mount {
                flags.push("auto");
            }
            // Short vault id — the name is the primary handle now.
            let vid = m.vault_id.to_string();
            let short = vid.get(..12).unwrap_or(&vid);
            t.add_row(vec![
                Cell::new(m.name.as_deref().unwrap_or("-")),
                Cell::new(&m.mountpoint),
                Cell::new(short),
                Cell::new(state),
                Cell::new(flags.join(",")),
            ]);
        }
        write!(f, "{t}")
    }
}
