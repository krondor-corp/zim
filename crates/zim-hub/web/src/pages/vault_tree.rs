//! The pack tree-pane: flat rows with depth padding, folder rows with
//! rotating chevrons, quiet file rows, a docked dashed "+ New" button,
//! and a right-click context menu for the destructive/secondary
//! actions. Markup and class names mirror
//! `pack/crates/app/templates/partials/tree_pane.html`.

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

/// What a context menu was opened on.
#[derive(Clone, PartialEq)]
pub enum CtxTarget {
    File(String),
    Folder(String),
    Root,
}

/// An open context menu: viewport position + target.
#[derive(Clone, PartialEq)]
pub struct CtxMenu {
    pub x: i32,
    pub y: i32,
    pub target: CtxTarget,
}

#[derive(Properties, PartialEq)]
pub struct TreePaneProps {
    pub rows: Vec<TreeNode>,
    /// The open file's path (active-row highlight).
    pub active: Option<String>,
    pub on_toggle: Callback<String>,
    pub on_open: Callback<String>,
    pub on_new_note: Callback<()>,
    pub on_upload: Callback<()>,
    /// Right-click anywhere in the tree → context menu.
    pub on_ctx: Callback<CtxMenu>,
}

#[function_component(TreePane)]
pub fn tree_pane(props: &TreePaneProps) -> Html {
    let root_ctx = {
        let on_ctx = props.on_ctx.clone();
        Callback::from(move |e: MouseEvent| {
            e.prevent_default();
            on_ctx.emit(CtxMenu {
                x: e.client_x(),
                y: e.client_y(),
                target: CtxTarget::Root,
            });
        })
    };
    let new_note = {
        let cb = props.on_new_note.clone();
        Callback::from(move |_: MouseEvent| cb.emit(()))
    };
    let upload = {
        let cb = props.on_upload.clone();
        Callback::from(move |_: MouseEvent| cb.emit(()))
    };
    html! {
        <aside id="tree-pane" class="tree-pane">
            <nav class="tree-pane__list" oncontextmenu={root_ctx}>
                { for props.rows.iter().map(|n| row(n, props)) }
            </nav>
            <div class="tree-pane__dock">
                <button type="button" class="tree-pane__new" onclick={new_note}>
                    { "+ New note" }
                </button>
                <button type="button" class="tree-pane__new" onclick={upload}>
                    { "\u{2191} Upload" }
                </button>
            </div>
        </aside>
    }
}

fn row(n: &TreeNode, props: &TreePaneProps) -> Html {
    let pad = format!("padding-left: {}px;", 8 + n.depth * 14);
    let active = props.active.as_deref() == Some(n.path.as_str());
    let oncontextmenu = {
        let on_ctx = props.on_ctx.clone();
        let target = if n.is_dir {
            CtxTarget::Folder(n.path.clone())
        } else {
            CtxTarget::File(n.path.clone())
        };
        Callback::from(move |e: MouseEvent| {
            e.prevent_default();
            e.stop_propagation();
            on_ctx.emit(CtxMenu {
                x: e.client_x(),
                y: e.client_y(),
                target: target.clone(),
            });
        })
    };

    if n.is_dir {
        let onclick = {
            let path = n.path.clone();
            let on_toggle = props.on_toggle.clone();
            Callback::from(move |e: MouseEvent| {
                e.prevent_default();
                on_toggle.emit(path.clone());
            })
        };
        html! {
            <div key={n.path.clone()}
                class={classes!("tree-item", "tree-item--folder",
                    (!n.expanded).then_some("tree-item--collapsed"))}
                style={pad} onclick={onclick} {oncontextmenu}>
                <button type="button" class="tree-item__chev" tabindex="-1"
                    aria-label={if n.expanded { "Collapse folder" } else { "Expand folder" }}>
                    <span class="chev-glyph">{ "\u{25BE}" }</span>
                </button>
                <span class="tree-item__name">{ n.name.clone() }</span>
            </div>
        }
    } else {
        let onclick = {
            let path = n.path.clone();
            let on_open = props.on_open.clone();
            Callback::from(move |e: MouseEvent| {
                e.prevent_default();
                on_open.emit(path.clone());
            })
        };
        html! {
            <a href="#" key={n.path.clone()}
               class={classes!("tree-item", "tree-item--file", active.then_some("tree-item--active"))}
               style={pad} {onclick} {oncontextmenu}>
                <span class="tree-item__name">{ n.name.clone() }</span>
            </a>
        }
    }
}
