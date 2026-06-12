use std::fmt;
use std::pin::Pin;

use async_trait::async_trait;
use clap::Args;
use zim_core::vault::VaultId;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::http_server::api::client::{ApiClient, ApiError};
use crate::http_server::api::v0::vault::ls::{EntryKind, LsRequest};

#[derive(Args, Debug, Clone)]
pub struct Ls {
    #[arg(skip)]
    pub target: String,
    #[arg(default_value = "/")]
    pub path: String,
    /// Recurse into subdirectories and render as a tree.
    #[arg(long)]
    pub deep: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct LsOutput {
    pub path: String,
    pub items: Vec<TreeNode>,
}

/// One filesystem entry — flat ls leaves `children` empty; `--deep`
/// populates it for every directory.
#[derive(Debug, serde::Serialize)]
pub struct TreeNode {
    pub name: String,
    pub kind: EntryKind,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreeNode>,
}

#[derive(Debug, thiserror::Error)]
pub enum LsError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Ls {
    type Context = ApiContext;
    type Output = LsOutput;
    type Error = LsError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let vault_id = ctx.client.resolve_vault(&self.target).await?;
        let path = super::normalize_path(&self.path);
        let items = ls_one(&ctx.client, vault_id, &path).await?;
        let items = if self.deep {
            expand(&ctx.client, vault_id, &path, items).await?
        } else {
            items
        };
        Ok(LsOutput { path, items })
    }
}

/// Single-directory listing — one HTTP round trip.
async fn ls_one(
    client: &ApiClient,
    vault_id: VaultId,
    path: &str,
) -> Result<Vec<TreeNode>, ApiError> {
    let r = client
        .call(LsRequest {
            vault_id,
            path: path.to_string(),
        })
        .await?;
    Ok(r.items
        .into_iter()
        .map(|p| TreeNode {
            name: p.name,
            kind: p.kind,
            children: Vec::new(),
        })
        .collect())
}

/// Recurse into every directory under `siblings`, returning the same
/// list with `children` populated. Boxed because `async fn` + direct
/// recursion isn't supported.
fn expand<'a>(
    client: &'a ApiClient,
    vault_id: VaultId,
    parent: &'a str,
    siblings: Vec<TreeNode>,
) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<TreeNode>, ApiError>> + Send + 'a>> {
    Box::pin(async move {
        let mut out = Vec::with_capacity(siblings.len());
        for node in siblings {
            let kids = if matches!(node.kind, EntryKind::Dir) {
                let sub = join_path(parent, &node.name);
                let raw = ls_one(client, vault_id, &sub).await?;
                expand(client, vault_id, &sub, raw).await?
            } else {
                Vec::new()
            };
            out.push(TreeNode {
                children: kids,
                ..node
            });
        }
        Ok(out)
    })
}

fn join_path(parent: &str, child: &str) -> String {
    if parent == "/" {
        format!("/{child}")
    } else {
        format!("{}/{}", parent.trim_end_matches('/'), child)
    }
}

impl fmt::Display for LsOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.items.is_empty() {
            return write!(f, "{} {}", ui::dim("empty"), ui::dim(self.path.as_str()));
        }
        // Flat mode: one entry per line, dirs marked with a trailing
        // slash and rendered as identifiers (cyan when colours are on).
        // Tree mode: same per-entry rendering plus branch glyphs.
        let any_deep = self.items.iter().any(|n| !n.children.is_empty());
        if any_deep {
            render_tree(f, &self.items, "")
        } else {
            for node in &self.items {
                writeln!(f, "{}", format_entry(node))?;
            }
            Ok(())
        }
    }
}

fn format_entry(node: &TreeNode) -> String {
    match node.kind {
        EntryKind::Dir => ui::ident(format!("{}/", node.name)),
        EntryKind::File => node.name.clone(),
    }
}

/// `tree`-style rendering with `├──` / `└──` branch characters.
fn render_tree(f: &mut fmt::Formatter<'_>, nodes: &[TreeNode], prefix: &str) -> fmt::Result {
    let last_ix = nodes.len().saturating_sub(1);
    for (i, node) in nodes.iter().enumerate() {
        let is_last = i == last_ix;
        let branch = if is_last { "└── " } else { "├── " };
        writeln!(f, "{prefix}{branch}{}", format_entry(node))?;
        if !node.children.is_empty() {
            let extension = if is_last { "    " } else { "│   " };
            let child_prefix = format!("{prefix}{extension}");
            render_tree(f, &node.children, &child_prefix)?;
        }
    }
    Ok(())
}
