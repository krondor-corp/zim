//! The pack tree-pane: flat rows with depth padding, folder rows with
//! rotating chevrons, quiet file rows, a docked dashed "+ New" button,
//! and a right-click (or, on touch screens, long-press) context menu for
//! the destructive/secondary actions. Markup and class names mirror
//! `pack/crates/app/templates/partials/tree_pane.html`.

use std::collections::BTreeSet;

use wasm_bindgen::JsCast;
use web_sys::{DragEvent, Element};
use yew::prelude::*;

use crate::components::icon::{icon, Icon};
use crate::components::long_press::{use_long_press, Held};
use zim_wasm::{DirEntry, EntryKind};

/// One rendered row. The tree is a FLAT vec — depth is presentation.
#[derive(Clone, PartialEq)]
pub struct TreeNode {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
    pub expanded: bool,
}

/// Build the flat row list by walking root + every expanded dir.
pub async fn build_rows<F, Fut>(ls: F, expanded: &BTreeSet<String>) -> Result<Vec<TreeNode>, String>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<DirEntry>, String>>,
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
    Fut: std::future::Future<Output = Result<Vec<DirEntry>, String>>,
{
    Box::pin(async move {
        let mut rows = Vec::new();
        let entries = ls(dir.to_string()).await?;
        for e in entries {
            let path = if dir == "/" {
                format!("/{}", e.name)
            } else {
                format!("{dir}/{}", e.name)
            };
            let is_dir = e.kind == EntryKind::Dir;
            let is_expanded = is_dir && expanded.contains(&path);
            rows.push(TreeNode {
                path: path.clone(),
                name: e.name,
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
    /// Right-click (long-press on touch) anywhere in the tree → context menu.
    pub on_ctx: Callback<CtxMenu>,
    /// Move `from` path into destination directory `into` (drag & drop).
    pub on_move: Callback<(String, String)>,
    /// The path currently being dragged (drives dim/drop-target styling).
    pub dragging: Option<String>,
    pub on_drag_start: Callback<String>,
    pub on_drag_end: Callback<()>,
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
    // A held finger opens the menu too. The row under it is found from
    // the touch target's `data-path`, so one handler covers the list.
    let press = use_long_press({
        let on_ctx = props.on_ctx.clone();
        Callback::from(move |h: Held| {
            on_ctx.emit(CtxMenu {
                x: h.x,
                y: h.y,
                target: ctx_target(h.target),
            })
        })
    });
    // Dragging rows is a mouse affair: on touch, a held row is the menu.
    let draggable = !coarse_pointer();
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
            <nav class="tree-pane__list" oncontextmenu={root_ctx}
                ontouchstart={press.start} ontouchmove={press.moved}
                ontouchend={press.end} ontouchcancel={press.cancel}
                ondragover={Callback::from(|e: DragEvent| e.prevent_default())}
                ondrop={
                    let on_move = props.on_move.clone();
                    let on_drag_end = props.on_drag_end.clone();
                    Callback::from(move |e: DragEvent| {
                        e.prevent_default();
                        if let Some(dt) = e.data_transfer() {
                            if let Ok(from) = dt.get_data("text/plain") {
                                if !from.is_empty() { on_move.emit((from, "/".to_string())); }
                            }
                        }
                        on_drag_end.emit(());
                    })
                }>
                { for props.rows.iter().map(|n| row(n, props, draggable)) }
            </nav>
            <div class="tree-pane__dock">
                <button type="button" class="tree-pane__new" onclick={new_note}>
                    { "+ New note" }
                </button>
                <button type="button" class="tree-pane__new" onclick={upload}>
                    { icon(Icon::Upload, None) }{ " Upload" }
                </button>
            </div>
        </aside>
    }
}

/// What a touch landed on: the row (by its `data-path`), else the list.
fn ctx_target(target: Option<web_sys::EventTarget>) -> CtxTarget {
    let row = target
        .and_then(|t| t.dyn_into::<Element>().ok())
        .and_then(|el| el.closest("[data-path]").ok().flatten());
    match row.and_then(|r| Some((r.get_attribute("data-path")?, r.has_attribute("data-dir")))) {
        Some((path, true)) => CtxTarget::Folder(path),
        Some((path, false)) => CtxTarget::File(path),
        None => CtxTarget::Root,
    }
}

/// A touch screen (finger, not mouse): no drag & drop, long-press menus.
fn coarse_pointer() -> bool {
    web_sys::window()
        .and_then(|w| w.match_media("(pointer: coarse)").ok().flatten())
        .is_some_and(|m| m.matches())
}

fn row(n: &TreeNode, props: &TreePaneProps, draggable: bool) -> Html {
    let draggable = if draggable { "true" } else { "false" };
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

    let dragging_this = props.dragging.as_deref() == Some(n.path.as_str());
    let ondragstart = {
        let path = n.path.clone();
        let on_drag_start = props.on_drag_start.clone();
        Callback::from(move |e: DragEvent| {
            if let Some(dt) = e.data_transfer() {
                let _ = dt.set_data("text/plain", &path);
                dt.set_effect_allowed("move");
            }
            on_drag_start.emit(path.clone());
        })
    };
    let ondragend = {
        let on_drag_end = props.on_drag_end.clone();
        Callback::from(move |_: DragEvent| on_drag_end.emit(()))
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
        // A folder can't be dropped into itself or its own descendants.
        let dragged = props.dragging.clone();
        let self_path = n.path.clone();
        let is_valid_drop = move |from: &str| {
            from != self_path
                && !self_path.starts_with(&format!("{from}/"))
                && crate::pages::vault_tree::parent_of(from) != self_path
        };
        let droppable = dragged.as_deref().map(is_valid_drop).unwrap_or(false);
        let ondragover = {
            Callback::from(move |e: DragEvent| {
                if droppable {
                    e.prevent_default();
                    if let Some(dt) = e.data_transfer() {
                        dt.set_drop_effect("move");
                    }
                }
            })
        };
        let ondrop = {
            let path = n.path.clone();
            let on_move = props.on_move.clone();
            let on_drag_end = props.on_drag_end.clone();
            Callback::from(move |e: DragEvent| {
                e.prevent_default();
                e.stop_propagation();
                if let Some(dt) = e.data_transfer() {
                    if let Ok(from) = dt.get_data("text/plain") {
                        if !from.is_empty() {
                            on_move.emit((from, path.clone()));
                        }
                    }
                }
                on_drag_end.emit(());
            })
        };
        html! {
            <div key={n.path.clone()} {draggable} data-path={n.path.clone()} data-dir=""
                class={classes!("tree-item", "tree-item--folder",
                    (!n.expanded).then_some("tree-item--collapsed"),
                    dragging_this.then_some("tree-item--dragging"))}
                style={pad} onclick={onclick} {oncontextmenu}
                {ondragstart} {ondragend} {ondragover} {ondrop}>
                <button type="button" class="tree-item__chev" tabindex="-1"
                    aria-label={if n.expanded { "Collapse folder" } else { "Expand folder" }}>
                    <span class="chev-glyph">{ icon(Icon::Chevron, None) }</span>
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
            <a href="#" key={n.path.clone()} {draggable} data-path={n.path.clone()}
               class={classes!("tree-item", "tree-item--file", active.then_some("tree-item--active"),
                   dragging_this.then_some("tree-item--dragging"))}
               style={pad} {onclick} {oncontextmenu} {ondragstart} {ondragend}>
                <span class="tree-item__name">{ n.name.clone() }</span>
            </a>
        }
    }
}

/// Parent directory of a vault path (`"/a/b"` → `"/a"`, `"/a"` → `"/"`).
pub fn parent_of(path: &str) -> String {
    match path.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(i) => path[..i].to_string(),
    }
}
