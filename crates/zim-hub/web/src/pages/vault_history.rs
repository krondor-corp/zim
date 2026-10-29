//! Version history: browse the vault's signed manifest chain and read any
//! version as it was. Three read-only columns — versions (newest first,
//! from the hub's `/log`), the picked version's tree, and a file preview
//! with "Restore this file". Reads go through a [`WasmVersion`] handle,
//! which has no `save`; restoring hands the old bytes to the parent, which
//! writes them forward on the live vault as a normal save. The chain is
//! append-only — nothing here rewrites it.

use std::cell::RefCell;
use std::rc::Rc;

use yew::prelude::*;
use zim_wasm::{WasmVersion, BLOB_PENDING};

use crate::api::{Device, FsEntry};
use crate::pages::vault_editor::OpenFile;
use crate::pages::vault_file::{
    digest, guess_mime, is_textual, object_url, SYNC_RETRIES, SYNC_RETRY_MS,
};
use crate::pages::vault_history_view::{
    author_label, file_preview, tree_list, version_row, VersionMeta,
};
use crate::util::{jserr, origin, sleep};

/// Versions fetched per page of the chain walk.
const PAGE: u32 = 20;

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
    /// Why the parent's last restore save failed; empty when it didn't.
    pub restore_error: String,
}

type Snapshot = Rc<RefCell<Option<Rc<WasmVersion>>>>;

/// The open snapshot, cloned out so no borrow is held across an await.
fn snapshot(snap: &Snapshot) -> Result<Rc<WasmVersion>, String> {
    snap.borrow()
        .clone()
        .ok_or_else(|| "no version open".into())
}

/// Read `path` from the snapshot, riding out a blob the hub hasn't
/// mirrored yet (the version was written by another device moments ago):
/// `on_pending` fires and the read is retried, as the live editor does.
/// `Ok(None)` when `still_wanted` turns false mid-wait.
async fn cat_synced(
    v: &WasmVersion,
    path: &str,
    still_wanted: impl Fn() -> bool,
    on_pending: impl Fn(),
) -> Result<Option<Vec<u8>>, String> {
    for attempt in 0..SYNC_RETRIES {
        if attempt > 0 {
            sleep(SYNC_RETRY_MS).await;
        }
        if !still_wanted() {
            return Ok(None);
        }
        match v.cat(path.to_string()).await {
            Ok(bytes) => return Ok(Some(bytes)),
            Err(e) if js_sys::Error::from(e.clone()).name() == BLOB_PENDING => on_pending(),
            Err(e) => return Err(jserr(e)),
        }
    }
    Err(format!(
        "{path} hasn't reached the hub yet \u{2014} try again in a moment"
    ))
}

async fn ls(v: &WasmVersion, dir: &str) -> Result<Vec<FsEntry>, String> {
    let json = v.ls(dir.to_string()).await.map_err(|e| jserr(e.into()))?;
    let mut entries: Vec<FsEntry> = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    entries.sort_by(|a, b| (a.kind != "dir", &a.name).cmp(&(b.kind != "dir", &b.name)));
    Ok(entries)
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
    // Bumped per file open and by navigation, which abandons a file read
    // still waiting on the hub.
    let file_seq = use_mut_ref(|| 0u32);
    let dir = use_state(|| "/".to_string());
    let entries = use_state(|| None::<Vec<FsEntry>>);
    let preview = use_state(|| None::<OpenFile>);
    // Whether the previewed file is the same revision at the live head
    // (nothing to restore). `None` when the comparison couldn't be made.
    let unchanged = use_state(|| None::<bool>);
    let error = use_state(String::new);
    // A file whose blob is still on its way to the hub; shown as
    // "syncing" while the read retries.
    let syncing = use_state(|| None::<String>);

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
        let file_seq = file_seq.clone();
        let preview = preview.clone();
        let syncing = syncing.clone();
        let error = error.clone();
        Callback::from(move |to: String| {
            let ticket = *seq.borrow();
            *file_seq.borrow_mut() += 1;
            syncing.set(None);
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
        let syncing = syncing.clone();
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
            syncing.set(None);
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
        let file_seq = file_seq.clone();
        let preview = preview.clone();
        let unchanged = unchanged.clone();
        let syncing = syncing.clone();
        let error = error.clone();
        Callback::from(move |path: String| {
            // Each open takes a fresh ticket so a slow (syncing) read of
            // another file can't land over this one.
            let ticket = {
                let mut s = file_seq.borrow_mut();
                *s += 1;
                *s
            };
            let snap = snap.clone();
            let file_seq = file_seq.clone();
            let preview = preview.clone();
            let unchanged = unchanged.clone();
            let syncing = syncing.clone();
            let error = error.clone();
            error.set(String::new());
            yew::platform::spawn_local(async move {
                let current = || *file_seq.borrow() == ticket;
                let loaded = async {
                    let v = snapshot(&snap)?;
                    let pending = || {
                        preview.set(None);
                        syncing.set(Some(path.clone()));
                    };
                    let Some(bytes) = cat_synced(&v, &path, current, pending).await? else {
                        return Ok(None);
                    };
                    let name = path.rsplit('/').next().unwrap_or_default();
                    let mime = guess_mime(name);
                    let url = object_url(&bytes, &mime)?;
                    let digest = digest(&bytes);
                    let text = (is_textual(&mime) && bytes.len() < 2_000_000)
                        .then(|| String::from_utf8(bytes).ok())
                        .flatten();
                    let file = OpenFile {
                        path: path.clone(),
                        mime,
                        url,
                        text,
                        digest,
                        rev: 0,
                    };
                    // Unknown on failure: Restore stays offered and
                    // re-checks on click.
                    let same = v.is_unchanged(path.clone()).await.ok();
                    Ok::<_, String>(Some((file, same)))
                }
                .await;
                if !current() {
                    return;
                }
                syncing.set(None);
                match loaded {
                    Ok(Some((f, same))) => {
                        unchanged.set(same);
                        preview.set(Some(f));
                    }
                    Ok(None) => {}
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let restore = {
        let snap = snap.clone();
        let selected = selected.clone();
        let unchanged = unchanged.clone();
        let syncing = syncing.clone();
        let error = error.clone();
        let on_restore = props.on_restore.clone();
        Callback::from(move |path: String| {
            let Some(height) = *selected else {
                return;
            };
            let snap = snap.clone();
            let unchanged = unchanged.clone();
            let syncing = syncing.clone();
            let error = error.clone();
            let on_restore = on_restore.clone();
            error.set(String::new());
            yew::platform::spawn_local(async move {
                // The user asked for this restore; it waits out a syncing
                // blob rather than being abandoned by later navigation.
                let bytes = async {
                    let v = snapshot(&snap)?;
                    // Re-check against the head *now*: the file may have
                    // changed (or been reverted) since it was opened. A
                    // file that is still this revision has nothing to
                    // restore — never write a no-op version.
                    if v.is_unchanged(path.clone())
                        .await
                        .map_err(|e| jserr(e.into()))?
                    {
                        unchanged.set(Some(true));
                        return Ok(None);
                    }
                    unchanged.set(Some(false));
                    let pending = || syncing.set(Some(path.clone()));
                    cat_synced(&v, &path, || true, pending).await
                }
                .await;
                syncing.set(None);
                match bytes {
                    Ok(None) => {}
                    Ok(Some(bytes)) => on_restore.emit(Restore {
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
                    if !props.restore_error.is_empty() {
                        <div class="error" style="margin:1rem 2rem 0;">{ props.restore_error.clone() }</div>
                    }
                    if !(*error).is_empty() {
                        <div class="error" style="margin:1rem 2rem 0;">{ (*error).clone() }</div>
                    }
                    {
                        match &*preview {
                            Some(f) => file_preview(f, *unchanged, picked.map(|v| v.height), &restore),
                            None => match &*syncing {
                                Some(path) => html! {
                                    <div class="doc-empty">
                                        { format!("Syncing {path}\u{2026} this version is still on its way to the hub.") }
                                    </div>
                                },
                                None => html! { <div class="doc-empty">{ "Pick a file to see it as it was." }</div> },
                            },
                        }
                    }
                </div>
            </div>
        </div>
    }
}
