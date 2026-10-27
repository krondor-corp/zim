//! Version history: browse the vault's signed manifest chain and read any
//! version as it was. Three read-only columns — versions (newest first,
//! from the hub's `/log`), the picked version's tree, and a file preview
//! with "Restore this file". Reads go through a [`WasmVersion`] handle,
//! which has no `save`; restoring hands the old bytes to the parent, which
//! writes them forward on the live vault as a normal save. The chain is
//! append-only — nothing here rewrites it.

use std::cell::RefCell;
use std::rc::Rc;

use serde::Deserialize;
use yew::prelude::*;
use zim_wasm::WasmVersion;

use crate::api::{Device, FsEntry};
use crate::pages::vault::{guess_mime, is_textual, object_url, own_label, short_id};
use crate::pages::vault_editor::{media_preview, OpenFile};
use crate::util::{jserr, origin};

/// Versions fetched per page of the chain walk.
const PAGE: u32 = 20;

/// One version as [`WasmVersion::history`] serializes it.
#[derive(Clone, PartialEq, Deserialize)]
struct VersionMeta {
    height: u64,
    manifest_hash: String,
    author: Option<String>,
    /// This browser's key holds a share in this version.
    readable: bool,
}

/// A historical file to write forward as the live vault's next version.
pub struct Restore {
    pub path: String,
    pub bytes: Vec<u8>,
    pub height: u64,
}

#[derive(Properties, PartialEq)]
pub struct HistoryProps {
    pub vault_id: String,
    /// The account's devices — labels version authors.
    pub devices: Vec<Device>,
    pub on_close: Callback<()>,
    pub on_restore: Callback<Restore>,
}

type Snapshot = Rc<RefCell<Option<Rc<WasmVersion>>>>;

/// The open snapshot, cloned out so no borrow is held across an await.
fn snapshot(snap: &Snapshot) -> Result<Rc<WasmVersion>, String> {
    snap.borrow()
        .clone()
        .ok_or_else(|| "no version open".into())
}

async fn ls(v: &WasmVersion, dir: &str) -> Result<Vec<FsEntry>, String> {
    let json = v.ls(dir.to_string()).await.map_err(|e| jserr(e.into()))?;
    let mut entries: Vec<FsEntry> = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    entries.sort_by(|a, b| (a.kind != "dir", &a.name).cmp(&(b.kind != "dir", &b.name)));
    Ok(entries)
}

fn author_label(author: Option<&str>, devices: &[Device]) -> String {
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

#[function_component(HistoryView)]
pub fn history_view(props: &HistoryProps) -> Html {
    let versions = use_state(Vec::<VersionMeta>::new);
    // Height the next "Load older" page starts at; None once genesis is in.
    let older = use_state(|| None::<u64>);
    let head = use_state(|| None::<u64>);
    let selected = use_state(|| None::<u64>);
    let snap: Snapshot = use_mut_ref(|| None);
    // Bumped per pick; a load holding an older ticket drops its result.
    let seq = use_mut_ref(|| 0u32);
    let dir = use_state(|| "/".to_string());
    let entries = use_state(|| None::<Vec<FsEntry>>);
    let preview = use_state(|| None::<OpenFile>);
    let error = use_state(String::new);

    let load_page = {
        let versions = versions.clone();
        let older = older.clone();
        let head = head.clone();
        let error = error.clone();
        let vault_id = props.vault_id.clone();
        Callback::from(move |from: Option<u64>| {
            let versions = versions.clone();
            let older = older.clone();
            let head = head.clone();
            let error = error.clone();
            let vault_id = vault_id.clone();
            yew::platform::spawn_local(async move {
                let page = WasmVersion::history(vault_id, origin(), from, PAGE)
                    .await
                    .map_err(|e| jserr(e.into()))
                    .and_then(|json| {
                        serde_json::from_str::<Vec<VersionMeta>>(&json).map_err(|e| e.to_string())
                    });
                match page {
                    Ok(page) => {
                        if from.is_none() {
                            head.set(page.first().map(|v| v.height));
                        }
                        older.set(page.last().and_then(|v| v.height.checked_sub(1)));
                        let mut all = if from.is_none() {
                            Vec::new()
                        } else {
                            (*versions).clone()
                        };
                        all.extend(page);
                        versions.set(all);
                    }
                    Err(e) => error.set(e),
                }
            });
        })
    };

    {
        let load_page = load_page.clone();
        use_effect_with(props.vault_id.clone(), move |_| {
            load_page.emit(None);
            || ()
        });
    }

    // Revoke the preview's object URL when it changes or we unmount.
    {
        let url = (*preview).as_ref().map(|p| p.url.clone());
        use_effect_with(url, move |url| {
            let url = url.clone();
            move || {
                if let Some(u) = url {
                    let _ = web_sys::Url::revoke_object_url(&u);
                }
            }
        });
    }

    let browse = {
        let snap = snap.clone();
        let seq = seq.clone();
        let dir = dir.clone();
        let entries = entries.clone();
        let preview = preview.clone();
        let error = error.clone();
        Callback::from(move |to: String| {
            let ticket = *seq.borrow();
            let snap = snap.clone();
            let seq = seq.clone();
            let dir = dir.clone();
            let entries = entries.clone();
            let error = error.clone();
            preview.set(None);
            yew::platform::spawn_local(async move {
                let listed = match snapshot(&snap) {
                    Ok(v) => ls(&v, &to).await,
                    Err(e) => Err(e),
                };
                if *seq.borrow() != ticket {
                    return;
                }
                match listed {
                    Ok(list) => {
                        dir.set(to);
                        entries.set(Some(list));
                    }
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let pick = {
        let snap = snap.clone();
        let seq = seq.clone();
        let selected = selected.clone();
        let entries = entries.clone();
        let preview = preview.clone();
        let error = error.clone();
        let browse = browse.clone();
        let vault_id = props.vault_id.clone();
        Callback::from(move |height: u64| {
            let ticket = {
                let mut s = seq.borrow_mut();
                *s += 1;
                *s
            };
            selected.set(Some(height));
            entries.set(None);
            preview.set(None);
            error.set(String::new());
            *snap.borrow_mut() = None;
            let snap = snap.clone();
            let seq = seq.clone();
            let error = error.clone();
            let browse = browse.clone();
            let vault_id = vault_id.clone();
            yew::platform::spawn_local(async move {
                let opened = WasmVersion::open(vault_id, height, origin()).await;
                if *seq.borrow() != ticket {
                    return;
                }
                match opened {
                    Ok(v) => {
                        *snap.borrow_mut() = Some(Rc::new(v));
                        browse.emit("/".to_string());
                    }
                    Err(e) => error.set(jserr(e.into())),
                }
            });
        })
    };

    let open_file = {
        let snap = snap.clone();
        let seq = seq.clone();
        let preview = preview.clone();
        let error = error.clone();
        Callback::from(move |path: String| {
            let ticket = *seq.borrow();
            let snap = snap.clone();
            let seq = seq.clone();
            let preview = preview.clone();
            let error = error.clone();
            yew::platform::spawn_local(async move {
                let loaded = async {
                    let v = snapshot(&snap)?;
                    let bytes = v.cat(path.clone()).await.map_err(jserr)?;
                    let name = path.rsplit('/').next().unwrap_or_default();
                    let mime = guess_mime(name);
                    let url = object_url(&bytes, &mime)?;
                    let text = (is_textual(&mime) && bytes.len() < 2_000_000)
                        .then(|| String::from_utf8(bytes).ok())
                        .flatten();
                    Ok::<_, String>(OpenFile {
                        path,
                        mime,
                        url,
                        text,
                        rev: 0,
                    })
                }
                .await;
                if *seq.borrow() != ticket {
                    return;
                }
                match loaded {
                    Ok(f) => preview.set(Some(f)),
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let restore = {
        let snap = snap.clone();
        let selected = selected.clone();
        let error = error.clone();
        let on_restore = props.on_restore.clone();
        Callback::from(move |path: String| {
            let Some(height) = *selected else {
                return;
            };
            let snap = snap.clone();
            let error = error.clone();
            let on_restore = on_restore.clone();
            yew::platform::spawn_local(async move {
                let bytes = match snapshot(&snap) {
                    Ok(v) => v.cat(path.clone()).await.map_err(jserr),
                    Err(e) => Err(e),
                };
                match bytes {
                    Ok(bytes) => on_restore.emit(Restore {
                        path,
                        bytes,
                        height,
                    }),
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let picked = (*selected).and_then(|h| versions.iter().find(|v| v.height == h));
    let close = {
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| on_close.emit(()))
    };

    html! {
        <div class="editor-layout history-layout">
            <aside class="tree-pane">
                <div class="history-pane__title">{ "Versions" }</div>
                <div class="tree-pane__list">
                    { for versions.iter().map(|v| version_row(v, *head, *selected, &props.devices, &pick)) }
                    if let Some(from) = *older {
                        <button type="button" class="tree-pane__new"
                            onclick={{ let load_page = load_page.clone(); move |_: MouseEvent| load_page.emit(Some(from)) }}>
                            { "Load older" }
                        </button>
                    }
                </div>
            </aside>
            <aside class="tree-pane">
                <div class="history-pane__title" title={(*dir).clone()}>{ (*dir).clone() }</div>
                <div class="tree-pane__list">
                    { tree_list(&dir, entries.as_ref(), preview.as_ref().map(|p| p.path.as_str()), &browse, &open_file) }
                </div>
            </aside>
            <div class="editor-main">
                <div class="history-banner">
                    <span style="flex:1;">
                        {
                            match picked {
                                Some(v) => format!(
                                    "Viewing version {} \u{2014} read-only \u{00B7} by {}",
                                    v.height,
                                    author_label(v.author.as_deref(), &props.devices)
                                ),
                                None => "Version history \u{2014} pick a version to browse it as it was".to_string(),
                            }
                        }
                    </span>
                    <button type="button" class="dialog__btn" onclick={close}>{ "Back to live" }</button>
                </div>
                <div class="editor-body">
                    if !(*error).is_empty() {
                        <div class="error" style="margin:1rem 2rem 0;">{ (*error).clone() }</div>
                    }
                    {
                        match &*preview {
                            Some(f) => file_preview(f, &restore),
                            None => html! { <div class="doc-empty">{ "Pick a file to see it as it was." }</div> },
                        }
                    }
                </div>
            </div>
        </div>
    }
}

fn version_row(
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
    let title = if v.readable {
        format!("manifest {}", v.manifest_hash)
    } else {
        "Sealed before this browser was granted access".to_string()
    };
    html! {
        <button type="button" {title} disabled={!v.readable} {onclick}
            class={classes!(
                "tree-item", "tree-item--file", "history-version",
                (selected == Some(height)).then_some("tree-item--active"),
                (!v.readable).then_some("history-version--sealed"),
            )}>
            <span class="history-version__height">{ format!("v{height}") }</span>
            <span class="history-version__author">{ author_label(v.author.as_deref(), devices) }</span>
            if head == Some(height) {
                <span class="device-row__badge">{ "current" }</span>
            }
        </button>
    }
}

fn tree_list(
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
                onclick={move |_: MouseEvent| browse.emit(to.clone())}>{ "\u{2190} .." }</button>
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

fn file_preview(f: &OpenFile, restore: &Callback<String>) -> Html {
    let name = f.path.rsplit('/').next().unwrap_or_default().to_string();
    let onclick = {
        let restore = restore.clone();
        let path = f.path.clone();
        Callback::from(move |_: MouseEvent| restore.emit(path.clone()))
    };
    html! {
        <div class="editor-container">
            <div class="editor-title-row">
                <h1 class="editor-title" style="flex:1;" title={f.path.clone()}>{ name }</h1>
                <button type="button" class="app-header__btn app-header__btn--save" {onclick}
                    title="Write this content forward as the vault's next version">
                    { "Restore this file" }
                </button>
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
