//! The expandable file tree (pack's tree-pane pattern): a flat list of
//! rows with depth padding; folders toggle open/closed, expansion state
//! survives rebuilds so mutations (save/upload/rm) refresh in place.

use std::collections::BTreeSet;

use yew::prelude::*;

use crate::api::FsEntry;

/// One rendered row. The tree is a FLAT vec — depth is presentation.
#[derive(Clone, PartialEq)]
pub struct TreeNode {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
    pub expanded: bool,
}

fn sorted(entries: &[FsEntry]) -> Vec<FsEntry> {
    let mut v = entries.to_vec();
    v.sort_by(|a, b| {
        if a.kind == b.kind {
            a.name.cmp(&b.name)
        } else if a.kind == "dir" {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });
    v
}

/// Build the flat row list by walking root + every expanded dir.
/// `ls` is an async closure over the open vault; expansion state is the
/// caller-held set of expanded dir paths.
pub async fn build_rows<F, Fut>(ls: F, expanded: &BTreeSet<String>) -> Result<Vec<TreeNode>, String>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<FsEntry>, String>>,
{
    walk(&ls, "/", 0, expanded).await
}

fn walk<'a, F, Fut>(
    ls: &'a F,
    dir: &'a str,
    depth: usize,
    expanded: &'a BTreeSet<String>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<TreeNode>, String>> + 'a>>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<FsEntry>, String>>,
{
    Box::pin(async move {
        let mut rows = Vec::new();
        let entries = ls(dir.to_string()).await?;
        for e in sorted(&entries) {
            let path = if dir == "/" {
                format!("/{}", e.name)
            } else {
                format!("{dir}/{}", e.name)
            };
            let is_dir = e.kind == "dir";
            let is_expanded = is_dir && expanded.contains(&path);
            rows.push(TreeNode {
                path: path.clone(),
                name: e.name.clone(),
                is_dir,
                depth,
                expanded: is_expanded,
            });
            if is_expanded {
                rows.extend(walk(ls, &path, depth + 1, expanded).await?);
            }
        }
        Ok(rows)
    })
}

#[derive(Properties, PartialEq)]
pub struct TreePaneProps {
    pub rows: Vec<TreeNode>,
    /// The open file's path (active-row highlight).
    pub active: Option<String>,
    pub on_toggle: Callback<String>,
    pub on_open: Callback<String>,
    pub on_remove: Callback<String>,
}

#[function_component(TreePane)]
pub fn tree_pane(props: &TreePaneProps) -> Html {
    html! {
        <nav class="tree-list">
            if props.rows.is_empty() {
                <p class="muted tree-empty">{ "(empty vault)" }</p>
            }
            { for props.rows.iter().map(|n| row(n, props)) }
        </nav>
    }
}

fn row(n: &TreeNode, props: &TreePaneProps) -> Html {
    let pad = format!("padding-left: {}px;", 8 + n.depth * 14);
    let active = props.active.as_deref() == Some(n.path.as_str());
    let onclick = {
        let path = n.path.clone();
        let is_dir = n.is_dir;
        let on_toggle = props.on_toggle.clone();
        let on_open = props.on_open.clone();
        Callback::from(move |e: MouseEvent| {
            e.prevent_default();
            if is_dir {
                on_toggle.emit(path.clone());
            } else {
                on_open.emit(path.clone());
            }
        })
    };
    let on_del = {
        let path = n.path.clone();
        let on_remove = props.on_remove.clone();
        Callback::from(move |e: MouseEvent| {
            e.prevent_default();
            e.stop_propagation();
            on_remove.emit(path.clone());
        })
    };
    html! {
        <a href="#" key={n.path.clone()}
           class={classes!("tree-item", active.then_some("tree-item--active"))}
           style={pad} {onclick}>
            if n.is_dir {
                <span class="tree-item__chev">{ if n.expanded { "\u{25BE}" } else { "\u{25B8}" } }</span>
            } else {
                <span class="tree-item__chev tree-item__chev--blank"></span>
            }
            <span class="tree-item__icon">{ if n.is_dir { "\u{1F4C1}" } else { "\u{1F4C4}" } }</span>
            <span class="tree-item__name">{ n.name.clone() }</span>
            <button type="button" class="tree-item__del" title="Delete" onclick={on_del}>{ "\u{2715}" }</button>
        </a>
    }
}
