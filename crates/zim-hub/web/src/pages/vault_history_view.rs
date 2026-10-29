//! Presentational pieces of the version-history view: the version list
//! rows, the read-only tree listing, and the file preview with its
//! Restore affordance. Pure render — state lives in
//! [`HistoryView`](super::vault_history::HistoryView).

use serde::Deserialize;
use yew::prelude::*;

use crate::api::{Device, FsEntry};
use crate::components::icon::{icon, Icon};
use crate::pages::vault::short_id;
use crate::pages::vault_details::own_label;
use crate::pages::vault_editor::{media_preview, OpenFile};

/// One version as [`WasmVersion::history`] serializes it.
#[derive(Clone, PartialEq, Deserialize)]
pub(crate) struct VersionMeta {
    pub height: u64,
    pub manifest_hash: String,
    pub author: Option<String>,
    /// This browser's key holds a share in this version.
    /// `"readable"` | `"sealed"` (no share for this browser — permanent) |
    /// `"unavailable"` (the hub didn't serve its manifest — maybe transient).
    pub access: String,
}

pub(crate) fn author_label(author: Option<&str>, devices: &[Device]) -> String {
    match author {
        None => "unknown author".to_string(),
        Some(a) => match devices.iter().find(|d| d.pubkey == a) {
            Some(d) => own_label(d),
            None => short_id(a),
        },
    }
}

fn parent(dir: &str) -> String {
    match dir.rsplit_once('/') {
        Some(("", _)) | None => "/".to_string(),
        Some((p, _)) => p.to_string(),
    }
}

fn child(dir: &str, name: &str) -> String {
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
}

pub(crate) fn version_row(
    v: &VersionMeta,
    head: Option<u64>,
    selected: Option<u64>,
    devices: &[Device],
    pick: &Callback<u64>,
) -> Html {
    let height = v.height;
    let onclick = {
        let pick = pick.clone();
        Callback::from(move |_: MouseEvent| pick.emit(height))
    };
    let sealed = v.access == "sealed";
    let unavailable = v.access == "unavailable";
    let title = if sealed {
        "Sealed before this browser was granted access".to_string()
    } else if unavailable {
        "Couldn't load this version from the hub \u{2014} click to try again".to_string()
    } else {
        format!("manifest {}", v.manifest_hash)
    };
    html! {
        <button type="button" {title} disabled={sealed} {onclick}
            class={classes!(
                "tree-item", "tree-item--file", "history-version",
                (selected == Some(height)).then_some("tree-item--active"),
                sealed.then_some("history-version--sealed"),
                unavailable.then_some("history-version--unavailable"),
            )}>
            <span class="history-version__height">{ format!("v{height}") }</span>
            <span class="history-version__author">
                { if unavailable { "couldn't load".to_string() } else { author_label(v.author.as_deref(), devices) } }
            </span>
            if head == Some(height) {
                <span class="device-row__badge">{ "current" }</span>
            }
        </button>
    }
}

pub(crate) fn tree_list(
    dir: &str,
    entries: Option<&Vec<FsEntry>>,
    active: Option<&str>,
    browse: &Callback<String>,
    open_file: &Callback<String>,
) -> Html {
    let Some(entries) = entries else {
        return html! {};
    };
    let up = (dir != "/").then(|| {
        let browse = browse.clone();
        let to = parent(dir);
        html! {
            <button type="button" class="tree-item tree-item--file history-entry"
                onclick={move |_: MouseEvent| browse.emit(to.clone())}>{ icon(Icon::Back, None) }{ " .." }</button>
        }
    });
    html! {
        <>
            { up.unwrap_or_default() }
            if entries.is_empty() {
                <div class="tree-item">{ "(empty)" }</div>
            }
            { for entries.iter().map(|e| {
                let path = child(dir, &e.name);
                let is_dir = e.kind == "dir";
                let onclick = {
                    let browse = browse.clone();
                    let open_file = open_file.clone();
                    let path = path.clone();
                    Callback::from(move |_: MouseEvent| {
                        if is_dir { browse.emit(path.clone()) } else { open_file.emit(path.clone()) }
                    })
                };
                html! {
                    <button type="button" {onclick}
                        class={classes!(
                            "tree-item", "tree-item--file", "history-entry",
                            (active == Some(path.as_str())).then_some("tree-item--active"),
                        )}>
                        { if is_dir { format!("{}/", e.name) } else { e.name.clone() } }
                    </button>
                }
            }) }
        </>
    }
}

/// `unchanged`: `Some(true)` when the file is the same revision at the
/// live head — Restore is withheld, since there's nothing to bring back.
pub(crate) fn file_preview(
    f: &OpenFile,
    unchanged: Option<bool>,
    height: Option<u64>,
    restore: &Callback<String>,
) -> Html {
    let name = f.path.rsplit('/').next().unwrap_or_default().to_string();
    let is_unchanged = unchanged == Some(true);
    let since = height
        .map(|h| format!(" since version {h}"))
        .unwrap_or_default();
    let onclick = {
        let restore = restore.clone();
        let path = f.path.clone();
        Callback::from(move |_: MouseEvent| restore.emit(path.clone()))
    };
    html! {
        <div class="editor-container">
            <div class="editor-title-row">
                <h1 class="editor-title" style="flex:1;" title={f.path.clone()}>{ name }</h1>
                if is_unchanged {
                    <span class="history-unchanged"
                        title="This file hasn't been rewritten since; the current version is identical">
                        { format!("Unchanged{since}") }
                    </span>
                } else {
                    <button type="button" class="app-header__btn app-header__btn--save" {onclick}
                        title="Write this content forward as the vault's next version">
                        { "Restore this file" }
                    </button>
                }
            </div>
            {
                match &f.text {
                    Some(text) => html! { <pre class="history-text">{ text.clone() }</pre> },
                    None => media_preview(f),
                }
            }
        </div>
    }
}
