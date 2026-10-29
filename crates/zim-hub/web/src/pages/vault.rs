//! Vault workspace: pack-style shell — persistent expandable file tree
//! on the left, document editor/viewer on the right. Every read and
//! write goes through the page's [`VaultBackend`]; text edits autosave
//! (see [`vault_autosave`](crate::pages::vault_autosave)).

use std::cell::Cell;
use std::collections::BTreeSet;
use std::rc::Rc;

use wasm_bindgen_futures::JsFuture;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_router::prelude::*;
use zim_hub_web::save_queue::SaveQueue;

use zim_core::vault::VaultId;
use zim_wasm::VaultInfo;

use crate::api::{fetch_devices, fetch_me, fetch_vaults, Device, VaultItem};
use crate::backend::{BackendState, ReadError, VaultBackend};
use crate::components::dialog::{ConfirmDialog, PromptDialog};
use crate::components::icon::{icon, Icon};
use crate::layouts::{menu_icon, HeaderMenu, SignOutItem};
use crate::pages::vault_autosave::{
    drain, settled, use_lifecycle_flush, Queue, SaveEvent, SaveState, AUTOSAVE_MS,
};
use crate::pages::vault_details::details_panel;
use crate::pages::vault_editor::EditorPane;
use crate::pages::vault_file::{load_file, OpenAction, OpenState, SYNC_RETRIES, SYNC_RETRY_MS};
use crate::pages::vault_history::{HistoryView, Restore};
use crate::pages::vault_palette::{use_palette_shortcut, Palette, PaletteAction};
use crate::pages::vault_tree::{build_rows, CtxMenu, CtxTarget, TreeNode, TreePane};
use crate::routes::Route;
use crate::util::{jserr, sleep};
use zim_hub_web::nav::shows_detail;

/// Which first-class dialog is open (replaces browser prompt/confirm).
#[derive(Clone, PartialEq)]
enum Dialog {
    NewNote { dir: String },
    NewFolder { dir: String },
    Delete { path: String },
    NewVault,
}

#[derive(Properties, PartialEq)]
pub struct Props {
    pub vault_id: VaultId,
    /// Signed-in user id — the switcher's "+ New vault" shares new
    /// vaults with all the account's devices.
    pub user_id: String,
}

/// Abbreviate a 64-char hex vault id for display: `01fc44f0\u{2026}405e38`.
/// The full id is in the Details panel.
pub(crate) fn short_id(id: &str) -> String {
    if id.len() > 20 {
        format!("{}\u{2026}{}", &id[..8], &id[id.len() - 6..])
    } else {
        id.to_string()
    }
}

/// How often an open vault re-checks the hub's head while the tab is
/// visible, so another device's edits show up without re-opening.
const FOLLOW_MS: i32 = 3_000;

/// Whether the tab is in the background (no point polling then).
fn tab_hidden() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .is_some_and(|d| d.hidden())
}

/// Join `dir` + `name` into a vault path.
fn joined(dir: &str, name: &str) -> String {
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// A `Callback<()>` that bumps a counter state — for effects that must
/// re-run with the *current* render's state when fired from an async task.
#[hook]
fn use_tick() -> (u32, Callback<()>) {
    let tick = use_state(|| 0u32);
    let count = use_mut_ref(|| 0u32);
    let bump = {
        let tick = tick.setter();
        Callback::from(move |_: ()| {
            let n = {
                let mut c = count.borrow_mut();
                *c = c.wrapping_add(1);
                *c
            };
            tick.set(n);
        })
    };
    (*tick, bump)
}

#[function_component(VaultTree)]
pub fn vault_tree(props: &Props) -> Html {
    let backend: VaultBackend = (*use_memo((), |_| VaultBackend::default())).clone();
    let expanded = use_state(BTreeSet::<String>::new);
    let rows = use_state(|| None::<Vec<TreeNode>>);
    let open_file = use_reducer(OpenState::default);
    // A file whose latest blob hasn't reached the hub yet; shown as
    // "syncing" (never as an editable empty file) while `on_open` retries.
    let syncing = use_state(|| None::<String>);
    // Bumped by every open; an in-flight load or syncing retry holding an
    // older ticket drops its result instead of clobbering a newer open.
    let open_seq = use_mut_ref(|| 0u32);
    let error = use_state(String::new);
    let status = use_state(String::new);
    // Unsaved edits, per path, and the one write in flight.
    let queue: Queue = use_mut_ref(SaveQueue::default);
    let save_state = use_state(|| SaveState::Idle);
    // Whether an autosave timer is pending; edits made while it is ride
    // along in its write.
    let timer_armed = use_mut_ref(|| false);
    let meta = use_state(|| None::<VaultInfo>);
    let devices = use_state(Vec::<Device>::new);
    let web_did = use_state(String::new);
    let show_details = use_state(|| false);
    let show_history = use_state(|| false);
    let show_palette = use_state(|| false);
    // A failed restore, shown inside the history view (the live error
    // banner is hidden under it) — the view stays open to retry.
    let restore_error = use_state(String::new);
    let vaults = use_state(Vec::<VaultItem>::new);
    let ctx = use_state(|| None::<CtxMenu>);
    let dialog = use_state(|| None::<Dialog>);
    let dragging = use_state(|| None::<String>);
    let upload_dir = use_state(|| "/".to_string());
    let upload_input = use_node_ref();

    // Rebuild the whole visible tree: refresh from the hub, walk root +
    // every expanded dir, refresh manifest metadata. `rebuild` only bumps
    // a tick; the effect runs with the current expansion set, so a
    // rebuild asked for from an old render's task never walks a stale one.
    let (rebuild_tick, rebuild) = use_tick();
    {
        let backend = backend.clone();
        let rows = rows.clone();
        let error = error.clone();
        let meta = meta.clone();
        use_effect_with(
            (rebuild_tick, (*expanded).clone()),
            move |(_, expanded_set)| {
                let expanded_set = expanded_set.clone();
                yew::platform::spawn_local(async move {
                    if backend.state() == BackendState::Closed {
                        return; // pre-open effect fire; mount path rebuilds explicitly
                    }
                    backend.refresh().await;
                    let lister = {
                        let backend = backend.clone();
                        move |p: String| {
                            let backend = backend.clone();
                            async move { backend.ls(p).await }
                        }
                    };
                    match build_rows(lister, &expanded_set).await {
                        Ok(r) => rows.set(Some(r)),
                        Err(e) => error.set(e),
                    }
                    if let Some(m) = backend.meta() {
                        meta.set(Some(m));
                    }
                });
                || ()
            },
        );
    }

    // --- autosave ---

    let on_save = {
        let open_file = open_file.dispatcher();
        let save_state = save_state.setter();
        let meta = meta.setter();
        let backend = backend.clone();
        Callback::from(move |ev: SaveEvent| match ev {
            SaveEvent::Saving => save_state.set(SaveState::Saving),
            SaveEvent::Saved { path, text } => {
                open_file.dispatch(OpenAction::Saved { path, text });
                if let Some(m) = backend.meta() {
                    meta.set(Some(m));
                }
                save_state.set(SaveState::Saved);
            }
            SaveEvent::Failed(e) => save_state.set(SaveState::Failed(e)),
        })
    };

    // Write every draft now (best effort; failures park them for the
    // next flush and show in the header).
    let flush = {
        let backend = backend.clone();
        let queue = queue.clone();
        let on_save = on_save.clone();
        Callback::from(move |_: ()| {
            queue.borrow_mut().flush();
            let backend = backend.clone();
            let queue = queue.clone();
            let on_save = on_save.clone();
            yew::platform::spawn_local(async move { drain(&backend, &queue, &on_save).await });
        })
    };
    use_lifecycle_flush(flush.clone(), queue.clone());
    // Leaving the page (another route) flushes too.
    {
        let flush = flush.clone();
        use_effect_with((), move |_| move || flush.emit(()));
    }

    // The editor streams its text; the page queues it and saves on a
    // timer armed by the first unsaved edit.
    let on_change = {
        let path = open_file.0.as_ref().map(|f| f.path.clone());
        let queue = queue.clone();
        let timer_armed = timer_armed.clone();
        let flush = flush.clone();
        let status = status.clone();
        let save_state = save_state.clone();
        Callback::from(move |text: String| {
            let Some(path) = path.clone() else {
                return;
            };
            queue.borrow_mut().edit(&path, text);
            if !status.is_empty() {
                status.set(String::new());
            }
            if *save_state == SaveState::Saved {
                save_state.set(SaveState::Idle);
            }
            if timer_armed.replace(true) {
                return;
            }
            let timer_armed = timer_armed.clone();
            let flush = flush.clone();
            yew::platform::spawn_local(async move {
                sleep(AUTOSAVE_MS).await;
                timer_armed.replace(false);
                flush.emit(());
            });
        })
    };

    // Open on mount, then build the root tree; load the account's vault
    // list (switcher) + devices (details panel).
    {
        let backend = backend.clone();
        let queue = queue.clone();
        let on_save = on_save.clone();
        let rebuild = rebuild.clone();
        let error = error.clone();
        let devices = devices.clone();
        let web_did = web_did.clone();
        let vaults = vaults.clone();
        let expanded = expanded.clone();
        let rows = rows.clone();
        let open_file = open_file.dispatcher();
        let syncing = syncing.clone();
        let open_seq = open_seq.clone();
        let show_history = show_history.clone();
        let vault_id = props.vault_id;
        use_effect_with(vault_id, move |_| {
            // Fresh vault, fresh view state: expansion paths and the
            // open file belong to the PREVIOUS vault.
            expanded.set(BTreeSet::new());
            show_history.set(false);
            rows.set(None);
            open_file.dispatch(OpenAction::Show(None));
            syncing.set(None);
            *open_seq.borrow_mut() += 1;
            crate::pages::workspace::remember_vault(&vault_id);
            yew::platform::spawn_local(async move {
                // Unsaved drafts belong to the previous vault: write them
                // there before it's swapped out.
                queue.borrow_mut().flush();
                drain(&backend, &queue, &on_save).await;
                settled(&queue).await;
                queue.borrow_mut().clear();
                match backend.open(vault_id).await {
                    Ok(()) => rebuild.emit(()),
                    Err(e) => error.set(e),
                }
                if let Ok(d) = fetch_devices().await {
                    devices.set(d);
                }
                if let Ok(Some(me)) = fetch_me().await {
                    web_did.set(me.did);
                }
                if let Ok(v) = fetch_vaults().await {
                    vaults.set(v);
                }
            });
            || ()
        });
    }

    // Revoke the open file's object URL when it changes or we unmount.
    {
        let url = open_file.0.as_ref().map(|p| p.url.clone());
        use_effect_with(url, move |url| {
            let url = url.clone();
            move || {
                if let Some(u) = url {
                    let _ = web_sys::Url::revoke_object_url(&u);
                }
            }
        });
    }

    let on_toggle = {
        let expanded = expanded.clone();
        Callback::from(move |path: String| {
            let mut next = (*expanded).clone();
            if !next.remove(&path) {
                next.insert(path);
            }
            expanded.set(next);
        })
    };

    // Open reads the hub's *current* head (another device may have
    // written since the last rebuild). A blob the hub is still mirroring
    // shows "syncing" and is retried; any failure clears the pane, so a
    // stale copy is never left up to be edited and saved over the remote
    // write. The file being left is flushed first; a file that still has
    // unsaved text (a write that failed) reopens with that text.
    let on_open = {
        let backend = backend.clone();
        let queue = queue.clone();
        let flush = flush.clone();
        let open_file = open_file.dispatcher();
        let syncing = syncing.clone();
        let open_seq = open_seq.clone();
        let error = error.clone();
        let status = status.clone();
        let rebuild = rebuild.clone();
        Callback::from(move |path: String| {
            flush.emit(());
            let ticket = {
                let mut seq = open_seq.borrow_mut();
                *seq += 1;
                *seq
            };
            let backend = backend.clone();
            let queue = queue.clone();
            let open_file = open_file.clone();
            let syncing = syncing.clone();
            let open_seq = open_seq.clone();
            let error = error.clone();
            let status = status.clone();
            let rebuild = rebuild.clone();
            yew::platform::spawn_local(async move {
                let current = || *open_seq.borrow() == ticket;
                for attempt in 0..SYNC_RETRIES {
                    if attempt > 0 {
                        sleep(SYNC_RETRY_MS).await;
                    }
                    if !current() {
                        return;
                    }
                    let changed = backend.refresh().await;
                    let loaded = load_file(&backend, path.clone()).await;
                    if changed {
                        rebuild.emit(());
                    }
                    if !current() {
                        return;
                    }
                    error.set(String::new());
                    status.set(String::new());
                    match loaded {
                        Ok(mut f) => {
                            if let Some(text) = queue.borrow().unsaved(&path) {
                                f.text = Some(text.to_string());
                            }
                            syncing.set(None);
                            open_file.dispatch(OpenAction::Show(Some(f)));
                            return;
                        }
                        Err(ReadError::Pending) => {
                            open_file.dispatch(OpenAction::Show(None));
                            syncing.set(Some(path.clone()));
                        }
                        Err(ReadError::Failed(e)) => {
                            open_file.dispatch(OpenAction::Show(None));
                            syncing.set(None);
                            error.set(e);
                            return;
                        }
                    }
                }
                syncing.set(None);
                error.set(format!(
                    "{path} hasn't reached the hub yet \u{2014} open it again in a moment"
                ));
            });
        })
    };

    // Follow remote edits: every FOLLOW_MS a ticker bumps `follow_tick`,
    // and the effect below (which sees this render's state) fast-forwards
    // to the hub's head. A changed head rebuilds the tree and reloads the
    // open file if its bytes changed — unless it holds unsaved text,
    // which is never clobbered. Our own autosaves don't count: they move
    // the local head too (no change to follow) and update the open file's
    // digest in place. A reload that couldn't land (blob still mirroring)
    // leaves `follow_stale` set so the next tick tries again.
    let (follow_tick, follow) = use_tick();
    let follow_stale = use_mut_ref(|| false);
    {
        use_effect_with(props.vault_id, move |_| {
            let alive = Rc::new(Cell::new(true));
            {
                let alive = alive.clone();
                yew::platform::spawn_local(async move {
                    loop {
                        sleep(FOLLOW_MS).await;
                        if !alive.get() {
                            return;
                        }
                        if !tab_hidden() {
                            follow.emit(());
                        }
                    }
                });
            }
            move || alive.set(false)
        });
    }
    {
        let backend = backend.clone();
        let queue = queue.clone();
        let open = open_file.0.clone();
        let open_file = open_file.dispatcher();
        let open_seq = open_seq.clone();
        let follow_stale = follow_stale.clone();
        let status = status.clone();
        let rebuild = rebuild.clone();
        use_effect_with(follow_tick, move |&tick| {
            // A user action or a save holds the handle: it refreshes on
            // its own.
            if tick > 0 && backend.state() == BackendState::Idle {
                let ticket = *open_seq.borrow();
                yew::platform::spawn_local(async move {
                    let changed = backend.refresh().await;
                    if changed {
                        rebuild.emit(());
                    }
                    if !changed && !*follow_stale.borrow() {
                        return;
                    }
                    *follow_stale.borrow_mut() = false;
                    let Some(open) = open else {
                        return;
                    };
                    let dirty = || queue.borrow().is_dirty(&open.path);
                    if dirty() {
                        status.set("changed on another device".to_string());
                        return;
                    }
                    let loaded = load_file(&backend, open.path.clone()).await;
                    // The user opened another file or started typing
                    // while we loaded: their view wins.
                    let current = *open_seq.borrow() == ticket && !dirty();
                    match loaded {
                        Ok(f) if current => open_file.dispatch(OpenAction::Remote(f)),
                        Ok(f) => {
                            let _ = web_sys::Url::revoke_object_url(&f.url);
                        }
                        Err(ReadError::Pending) => *follow_stale.borrow_mut() = true,
                        // Removed or renamed elsewhere: don't leave a
                        // stale copy up to be saved back.
                        Err(ReadError::Failed(_)) if current => {
                            open_file.dispatch(OpenAction::Removed(open.path.clone()))
                        }
                        Err(ReadError::Failed(_)) => {}
                    }
                });
            }
            || ()
        });
    }

    // --- creation / destruction (ctx menu + docked New) ---

    let new_note_in = {
        let backend = backend.clone();
        let flush = flush.clone();
        let open_file = open_file.dispatcher();
        let syncing = syncing.clone();
        let open_seq = open_seq.clone();
        let error = error.clone();
        let status = status.clone();
        let rebuild = rebuild.clone();
        Callback::from(move |(dir, mut name): (String, String)| {
            if !name.to_ascii_lowercase().ends_with(".md") {
                name.push_str(".md");
            }
            flush.emit(());
            let backend = backend.clone();
            let path = joined(&dir, &name);
            let open_file = open_file.clone();
            let syncing = syncing.clone();
            let open_seq = open_seq.clone();
            let error = error.clone();
            let status = status.clone();
            let rebuild = rebuild.clone();
            status.set(format!("creating {name}\u{2026}"));
            yew::platform::spawn_local(async move {
                match backend.write(path.clone(), Vec::new()).await {
                    Ok(()) => {
                        status.set(String::new());
                        if let Ok(f) = load_file(&backend, path).await {
                            *open_seq.borrow_mut() += 1;
                            syncing.set(None);
                            open_file.dispatch(OpenAction::Show(Some(f)));
                        }
                        rebuild.emit(());
                    }
                    Err(e) => {
                        status.set(String::new());
                        error.set(e);
                    }
                }
            });
        })
    };

    let new_folder_in = {
        let backend = backend.clone();
        let error = error.clone();
        let status = status.clone();
        let rebuild = rebuild.clone();
        Callback::from(move |(dir, name): (String, String)| {
            let backend = backend.clone();
            let path = joined(&dir, &name);
            let error = error.clone();
            let status = status.clone();
            let rebuild = rebuild.clone();
            status.set(format!("creating {name}\u{2026}"));
            yew::platform::spawn_local(async move {
                match backend.mkdir(path).await {
                    Ok(()) => {
                        status.set(String::new());
                        rebuild.emit(());
                    }
                    Err(e) => {
                        status.set(String::new());
                        error.set(e);
                    }
                }
            });
        })
    };

    // Deleting drops the path's unsaved text and waits out a write in
    // flight, so autosave can't recreate what was just removed.
    let remove_path = {
        let backend = backend.clone();
        let queue = queue.clone();
        let open_file = open_file.dispatcher();
        let error = error.clone();
        let status = status.clone();
        let rebuild = rebuild.clone();
        Callback::from(move |path: String| {
            let name = path.rsplit('/').next().unwrap_or_default().to_string();
            queue.borrow_mut().discard(&path);
            let backend = backend.clone();
            let queue = queue.clone();
            let open_file = open_file.clone();
            let error = error.clone();
            let status = status.clone();
            let rebuild = rebuild.clone();
            status.set(format!("deleting {name}\u{2026}"));
            yew::platform::spawn_local(async move {
                settled(&queue).await;
                match backend.rm(path.clone()).await {
                    Ok(()) => {
                        status.set(String::new());
                        open_file.dispatch(OpenAction::Removed(path));
                        rebuild.emit(());
                    }
                    Err(e) => {
                        status.set(String::new());
                        error.set(e);
                    }
                }
            });
        })
    };

    let upload_files = {
        let backend = backend.clone();
        let error = error.clone();
        let status = status.clone();
        let rebuild = rebuild.clone();
        let upload_dir = upload_dir.clone();
        Callback::from(move |e: Event| {
            let input: HtmlInputElement = e.target_unchecked_into();
            let Some(files) = input.files() else {
                return;
            };
            // Snapshot the File objects BEFORE clearing the input — `files` is
            // a live FileList tied to the input, so `set_value("")` would empty
            // it out from under the async loop.
            let picked: Vec<web_sys::File> =
                (0..files.length()).filter_map(|i| files.item(i)).collect();
            input.set_value("");
            if picked.is_empty() {
                return;
            }
            let backend = backend.clone();
            let dir = (*upload_dir).clone();
            let error = error.clone();
            let status = status.clone();
            let rebuild = rebuild.clone();
            yew::platform::spawn_local(async move {
                for file in picked {
                    let name = file.name();
                    status.set(format!("uploading {name}\u{2026}"));
                    let bytes = match JsFuture::from(file.array_buffer()).await {
                        Ok(b) => js_sys::Uint8Array::new(&b).to_vec(),
                        Err(e) => {
                            status.set(String::new());
                            error.set(jserr(e));
                            return;
                        }
                    };
                    if let Err(e) = backend.write(joined(&dir, &name), bytes).await {
                        status.set(String::new());
                        error.set(e);
                        return;
                    }
                }
                status.set(String::new());
                rebuild.emit(());
            });
        })
    };

    // Drag & drop: move `from` into directory `into` (append its name).
    // Pending text is written first, so it lands before the move.
    let on_move = {
        let backend = backend.clone();
        let queue = queue.clone();
        let flush = flush.clone();
        let open_file = open_file.clone();
        let error = error.clone();
        let status = status.clone();
        let rebuild = rebuild.clone();
        Callback::from(move |(from, into): (String, String)| {
            let name = from.rsplit('/').next().unwrap_or_default().to_string();
            let to = joined(&into, &name);
            if to == from {
                return;
            }
            flush.emit(());
            let follows_open = open_file.0.as_ref().is_some_and(|f| f.path == from);
            let backend = backend.clone();
            let queue = queue.clone();
            let open_file = open_file.dispatcher();
            let error = error.clone();
            let status = status.clone();
            let rebuild = rebuild.clone();
            status.set(format!("moving {name}\u{2026}"));
            yew::platform::spawn_local(async move {
                settled(&queue).await;
                match backend.mv(from.clone(), to.clone()).await {
                    Ok(()) => {
                        status.set(String::new());
                        // If the open file moved, follow it to the new path.
                        if follows_open {
                            if let Ok(file) = load_file(&backend, to).await {
                                open_file.dispatch(OpenAction::Moved { from, file });
                            }
                        }
                        rebuild.emit(());
                    }
                    Err(e) => {
                        status.set(String::new());
                        error.set(e);
                    }
                }
            });
        })
    };
    let on_drag_start = {
        let dragging = dragging.clone();
        Callback::from(move |path: String| dragging.set(Some(path)))
    };
    let on_drag_end = {
        let dragging = dragging.clone();
        Callback::from(move |_: ()| dragging.set(None))
    };

    // Context menu plumbing: open from the tree, act + close here.
    let on_ctx = {
        let ctx = ctx.clone();
        Callback::from(move |m: CtxMenu| ctx.set(Some(m)))
    };
    let close_ctx = {
        let ctx = ctx.clone();
        Callback::from(move |_: MouseEvent| ctx.set(None))
    };

    let ctx_menu_html = (*ctx).as_ref().map(|m| {
        let style = format!("left:{}px;top:{}px;", m.x, m.y);
        let dir = match &m.target {
            CtxTarget::Folder(p) => p.clone(),
            _ => "/".to_string(),
        };
        let item = |label: &str, action: Callback<MouseEvent>| {
            html! { <button type="button" class="ctx-menu__item" onclick={action}>{ label }</button> }
        };
        let trigger_upload = {
            let upload_dir = upload_dir.clone();
            let upload_input = upload_input.clone();
            let ctx = ctx.clone();
            let dir = dir.clone();
            Callback::from(move |_: MouseEvent| {
                upload_dir.set(dir.clone());
                ctx.set(None);
                if let Some(input) = upload_input.cast::<HtmlInputElement>() {
                    input.click();
                }
            })
        };
        html! {
            <>
                <div style="position:fixed;inset:0;z-index:79;" onclick={close_ctx.clone()}
                     oncontextmenu={close_ctx.clone()}></div>
                <div class="ctx-menu" style={style}>
                    { item("New note", {
                        let dialog = dialog.clone();
                        let ctx2 = ctx.clone();
                        let dir = dir.clone();
                        Callback::from(move |_: MouseEvent| {
                            ctx2.set(None);
                            dialog.set(Some(Dialog::NewNote { dir: dir.clone() }));
                        })
                    }) }
                    { item("New folder", {
                        let dialog = dialog.clone();
                        let ctx2 = ctx.clone();
                        let dir = dir.clone();
                        Callback::from(move |_: MouseEvent| {
                            ctx2.set(None);
                            dialog.set(Some(Dialog::NewFolder { dir: dir.clone() }));
                        })
                    }) }
                    { item("Upload here", trigger_upload) }
                    {
                        match &m.target {
                            CtxTarget::File(p) | CtxTarget::Folder(p) => {
                                let dialog = dialog.clone();
                                let ctx2 = ctx.clone();
                                let p = p.clone();
                                html! { { item("Delete", Callback::from(move |_: MouseEvent| {
                                    ctx2.set(None);
                                    dialog.set(Some(Dialog::Delete { path: p.clone() }));
                                })) } }
                            }
                            CtxTarget::Root => html! {},
                        }
                    }
                </div>
            </>
        }
    });

    let navigator = use_navigator().expect("router context");
    let new_vault = {
        let dialog = dialog.clone();
        Callback::from(move |_: MouseEvent| dialog.set(Some(Dialog::NewVault)))
    };

    let vault_title = match &*meta {
        Some(m) if !m.name.is_empty() => m.name.clone(),
        _ => short_id(&props.vault_id.to_string()),
    };
    let active = open_file
        .0
        .as_ref()
        .map(|f| f.path.clone())
        .or_else(|| (*syncing).clone());

    // Breadcrumb: [vault switcher] / file / path / segments
    let crumbs = {
        let mut items: Vec<Html> = Vec::new();
        items.push(html! {
            <HeaderMenu class={classes!("vault-switch")}
                trigger_class={classes!("app-header__crumb", "app-header__crumb--current", "vault-switch__trigger")}
                trigger={html! { <>{ vault_title.clone() }{ icon(Icon::Chevron, None) }</> }}>
                { for vaults.iter().filter_map(|v| Some((v.vault_id.parse::<VaultId>().ok()?, v))).map(|(id, v)| {
                    let name = if v.name.is_empty() { short_id(&v.vault_id) } else { v.name.clone() };
                    html! {
                        <Link<Route> to={Route::Vault { id }} classes="app-menu__item">
                            { name }
                        </Link<Route>>
                    }
                }) }
                <button type="button" class="app-menu__item" onclick={new_vault.clone()}
                    style="background:none;border:none;width:100%;cursor:pointer;">
                    { "+ New vault" }
                </button>
            </HeaderMenu>
        });
        if let Some(f) = &open_file.0 {
            for seg in f.path.trim_start_matches('/').split('/') {
                items.push(html! { <span class="app-header__sep">{ "/" }</span> });
                items.push(html! { <span class="app-header__crumb app-header__crumb--current">{ seg.to_string() }</span> });
            }
        }
        items
    };

    // Narrow screens show the tree OR the open file (CSS keys off the
    // `--detail` modifier); the header back control returns to the tree.
    let detail = shows_detail(open_file.0.is_some(), syncing.is_some(), !error.is_empty());
    let on_back = {
        let flush = flush.clone();
        let open_file = open_file.dispatcher();
        let syncing = syncing.clone();
        let open_seq = open_seq.clone();
        let error = error.clone();
        Callback::from(move |_: MouseEvent| {
            flush.emit(());
            *open_seq.borrow_mut() += 1;
            open_file.dispatch(OpenAction::Show(None));
            syncing.set(None);
            error.set(String::new());
        })
    };

    let toggle_details = {
        let show_details = show_details.clone();
        Callback::from(move |_: MouseEvent| show_details.set(!*show_details))
    };

    let toggle_history = {
        let show_history = show_history.clone();
        let restore_error = restore_error.clone();
        let flush = flush.clone();
        Callback::from(move |_: MouseEvent| {
            flush.emit(());
            restore_error.set(String::new());
            show_history.set(!*show_history);
        })
    };

    // Cmd/Ctrl+K (or the header button) opens the palette; its choices
    // run through the same paths as the tree and header.
    {
        let show_palette = show_palette.setter();
        use_palette_shortcut(Callback::from(move |_: ()| show_palette.set(true)));
    }
    let on_palette_action = {
        let dialog = dialog.clone();
        let flush = flush.clone();
        let show_history = show_history.clone();
        let restore_error = restore_error.clone();
        let show_details = show_details.clone();
        let navigator = navigator.clone();
        let open_path = open_file.0.as_ref().map(|f| f.path.clone());
        Callback::from(move |action: PaletteAction| match action {
            PaletteAction::NewNote => dialog.set(Some(Dialog::NewNote {
                dir: "/".to_string(),
            })),
            PaletteAction::NewFolder => dialog.set(Some(Dialog::NewFolder {
                dir: "/".to_string(),
            })),
            PaletteAction::DeleteOpenFile => {
                if let Some(path) = open_path.clone() {
                    dialog.set(Some(Dialog::Delete { path }));
                }
            }
            PaletteAction::SaveNow => flush.emit(()),
            PaletteAction::History => {
                flush.emit(());
                restore_error.set(String::new());
                show_history.set(true);
            }
            PaletteAction::Details => show_details.set(true),
            PaletteAction::SwitchVault(id) => navigator.push(&Route::Vault { id }),
        })
    };

    // Restore = write the historical bytes forward as a normal save (the
    // chain only ever grows), then return to the live tree on that file.
    let on_restore = {
        let backend = backend.clone();
        let queue = queue.clone();
        let show_history = show_history.clone();
        let restore_error = restore_error.clone();
        let status = status.clone();
        let rebuild = rebuild.clone();
        let on_open = on_open.clone();
        Callback::from(move |r: Restore| {
            let name = r.path.rsplit('/').next().unwrap_or_default().to_string();
            // The restored bytes win over any text still parked for the file.
            queue.borrow_mut().discard(&r.path);
            let backend = backend.clone();
            let queue = queue.clone();
            let show_history = show_history.clone();
            let restore_error = restore_error.clone();
            let status = status.clone();
            let rebuild = rebuild.clone();
            let on_open = on_open.clone();
            restore_error.set(String::new());
            status.set(format!("restoring {name}\u{2026}"));
            yew::platform::spawn_local(async move {
                settled(&queue).await;
                match backend.write(r.path.clone(), r.bytes).await {
                    Ok(()) => {
                        status.set(format!("restored {name} from version {}", r.height));
                        show_history.set(false);
                        rebuild.emit(());
                        on_open.emit(r.path);
                    }
                    Err(e) => {
                        status.set(String::new());
                        restore_error.set(format!("Couldn't restore {name}: {e}"));
                    }
                }
            });
        })
    };

    let details_modal = if *show_details {
        let close = {
            let show_details = show_details.clone();
            Callback::from(move |_: MouseEvent| show_details.set(false))
        };
        html! {
            <>
                <div class="dialog-backdrop" onclick={close.clone()}></div>
                <div class="dialog dialog--wide" role="dialog" aria-modal="true">
                    <div class="dialog__form">
                        <div style="display:flex;align-items:center;gap:0.75rem;">
                            <h2 class="dialog__title" style="flex:1;margin:0;">{ "Vault details" }</h2>
                            <button type="button" class="dialog__btn" onclick={close}>{ "Close" }</button>
                        </div>
                        {
                            match &*meta {
                                None => html! { <div class="vault-details muted">{ "Loading\u{2026}" }</div> },
                                Some(m) => details_panel(m, &devices, &web_did),
                            }
                        }
                    </div>
                </div>
            </>
        }
    } else {
        Html::default()
    };

    html! {
        <>
            <header class={classes!("app-header", (detail && !*show_history).then_some("app-header--detail"))}>
                <HeaderMenu trigger={menu_icon()}>
                    <Link<Route> to={Route::Settings} classes="app-menu__item">{ "Settings" }</Link<Route>>
                    <SignOutItem />
                </HeaderMenu>
                <button type="button" class="app-header__back" onclick={on_back}>
                    { icon(Icon::Back, Some("Back to files")) }
                </button>
                <div class="app-header__slot app-header__slot--left">{ crumbs }</div>
                <div class="app-header__slot app-header__slot--right">
                    <span id="save-status" class="app-header__status">{ (*status).clone() }</span>
                    { save_indicator(&save_state) }
                    <button type="button" class="app-header__btn app-header__btn--palette"
                        title="Command palette (Ctrl/Cmd+K)" aria-label="Command palette"
                        onclick={{
                            let show_palette = show_palette.clone();
                            Callback::from(move |_: MouseEvent| show_palette.set(true))
                        }}>
                        { "\u{2315}" }
                    </button>
                    <button type="button"
                        class={classes!("app-header__btn", show_history.then_some("app-header__btn--on"))}
                        title="Version history" onclick={toggle_history}>
                        { "History" }
                    </button>
                    <button type="button" class="app-header__btn" title="Vault details" onclick={toggle_details}>
                        { icon(Icon::Info, Some("Vault details")) }
                    </button>
                </div>
            </header>

            if *show_history {
                <HistoryView vault_id={props.vault_id} devices={(*devices).clone()}
                    restore_error={(*restore_error).clone()}
                    on_close={{
                        let show_history = show_history.clone();
                        let restore_error = restore_error.clone();
                        Callback::from(move |_: ()| {
                            restore_error.set(String::new());
                            show_history.set(false);
                        })
                    }}
                    {on_restore} />
            }
            // Hidden, not unmounted, under history: the editor keeps its
            // textarea state (cursor, scroll) for when the user comes back.
            <div class={classes!("editor-layout", detail.then_some("editor-layout--detail"))}
                style={show_history.then_some("display:none;")}>
                {
                    match &*rows {
                        None => html! {
                            <aside class="tree-pane"><div class="doc-empty">{ "Loading\u{2026}" }</div></aside>
                        },
                        Some(r) => html! {
                            <TreePane rows={r.clone()} active={active.clone()}
                                on_toggle={on_toggle} on_open={on_open.clone()}
                                on_new_note={
                                    let dialog = dialog.clone();
                                    Callback::from(move |_: ()| dialog.set(Some(Dialog::NewNote { dir: "/".to_string() })))
                                }
                                on_upload={
                                    let upload_dir = upload_dir.clone();
                                    let upload_input = upload_input.clone();
                                    Callback::from(move |_: ()| {
                                        upload_dir.set("/".to_string());
                                        if let Some(input) = upload_input.cast::<HtmlInputElement>() {
                                            input.click();
                                        }
                                    })
                                }
                                on_ctx={on_ctx}
                                on_move={on_move}
                                dragging={(*dragging).clone()}
                                on_drag_start={on_drag_start}
                                on_drag_end={on_drag_end} />
                        },
                    }
                }
                <div class="editor-main">
                    <div class="editor-body">
                        if !(*error).is_empty() {
                            <div class="error" style="margin:1rem 2rem 0;">{ (*error).clone() }</div>
                        }
                        {
                            match &open_file.0 {
                                Some(f) => html! {
                                    <EditorPane file={f.clone()} on_change={on_change.clone()} />
                                },
                                None => match &*syncing {
                                    Some(path) => html! {
                                        <div class="doc-empty">
                                            { format!("Syncing {path}\u{2026} its latest version is still on its way to the hub.") }
                                        </div>
                                    },
                                    None => html! {
                                        <div class="doc-empty">
                                            { "Select a file, or right-click the tree for more." }
                                        </div>
                                    },
                                },
                            }
                        }
                    </div>
                </div>
            </div>

            <input type="file" multiple={true} style="display:none;"
                ref={upload_input} onchange={upload_files} />

            { ctx_menu_html.unwrap_or_default() }
            { details_modal }
            if *show_palette {
                <Palette backend={backend.clone()} vault_id={props.vault_id}
                    vaults={(*vaults).clone()}
                    open_path={open_file.0.as_ref().map(|f| f.path.clone())}
                    on_action={on_palette_action}
                    on_open={on_open.clone()}
                    on_close={{
                        let show_palette = show_palette.clone();
                        Callback::from(move |_: ()| show_palette.set(false))
                    }} />
            }
            {
                match &*dialog {
                    None => html! {},
                    Some(d) => {
                        let close = {
                            let dialog = dialog.clone();
                            Callback::from(move |_: ()| dialog.set(None))
                        };
                        match d.clone() {
                            Dialog::NewNote { dir } => {
                                let dialog = dialog.clone();
                                let new_note_in = new_note_in.clone();
                                html! {
                                    <PromptDialog title="New note" label="Name" value="note.md"
                                        on_cancel={close}
                                        on_submit={Callback::from(move |name: String| {
                                            dialog.set(None);
                                            new_note_in.emit((dir.clone(), name));
                                        })} />
                                }
                            }
                            Dialog::NewFolder { dir } => {
                                let dialog = dialog.clone();
                                let new_folder_in = new_folder_in.clone();
                                html! {
                                    <PromptDialog title="New folder" label="Name"
                                        on_cancel={close}
                                        on_submit={Callback::from(move |name: String| {
                                            dialog.set(None);
                                            new_folder_in.emit((dir.clone(), name));
                                        })} />
                                }
                            }
                            Dialog::Delete { path } => {
                                let dialog = dialog.clone();
                                let remove_path = remove_path.clone();
                                let name = path.rsplit('/').next().unwrap_or_default().to_string();
                                html! {
                                    <ConfirmDialog title={format!("Delete {name}?")}
                                        body="This removes it from the vault's current version. Prior versions keep their history."
                                        on_cancel={close}
                                        on_confirm={Callback::from(move |_| {
                                            dialog.set(None);
                                            remove_path.emit(path.clone());
                                        })} />
                                }
                            }
                            Dialog::NewVault => {
                                let dialog = dialog.clone();
                                let user_id = props.user_id.clone();
                                let navigator = navigator.clone();
                                let error = error.clone();
                                html! {
                                    <PromptDialog title="New vault" label="Name"
                                        on_cancel={close}
                                        on_submit={Callback::from(move |name: String| {
                                            dialog.set(None);
                                            let error = error.clone();
                                            crate::pages::workspace::create_vault(
                                                name,
                                                user_id.clone(),
                                                navigator.clone(),
                                                Callback::from(move |e: String| error.set(e)),
                                            );
                                        })} />
                                }
                            }
                        }
                    }
                }
            }
        </>
    }
}

/// The header's autosave indicator: "Saving…", a "Saved" that fades out,
/// or why the last write failed (its text stays queued for the next).
fn save_indicator(state: &SaveState) -> Html {
    match state {
        SaveState::Idle => Html::default(),
        SaveState::Saving => html! {
            <span class="app-header__status">{ "Saving\u{2026}" }</span>
        },
        SaveState::Saved => html! {
            <span class="app-header__status app-header__status--fade">{ "Saved" }</span>
        },
        SaveState::Failed(e) => html! {
            <span class="app-header__status app-header__status--error" title={e.clone()}>
                { "Not saved \u{2014} retrying on your next edit" }
            </span>
        },
    }
}
