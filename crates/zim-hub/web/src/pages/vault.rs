//! Vault workspace: pack-style shell — persistent expandable file tree
//! on the left, document editor/viewer on the right. Read (`ls`/`cat`)
//! and edit (`add_file`/`mkdir`/`rm` then `save`) go through the held
//! `WasmFs` — all blob crypto happens inside the SDK. The handle lives
//! in a `RefCell` across user actions (which are sequential, so the
//! borrows don't overlap).

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use serde::Deserialize;
use wasm_bindgen_futures::JsFuture;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_router::prelude::*;
use zim_wasm::WasmFs;

use crate::api::{fetch_devices, fetch_me, fetch_vaults, Device, FsEntry, VaultItem};
use crate::pages::vault_editor::{EditorPane, OpenFile};
use crate::pages::vault_tree::{build_rows, CtxMenu, CtxTarget, TreeNode, TreePane};
use crate::layouts::HeaderMenu;
use crate::routes::Route;
use crate::util::{jserr, origin};

type FsHandle = Rc<RefCell<Option<WasmFs>>>;

/// Vault manifest metadata for the Details panel — parsed from
/// [`WasmFs::manifest_info`].
#[derive(Clone, PartialEq, Deserialize)]
struct VaultMeta {
    vault_id: String,
    name: String,
    height: u64,
    manifest_hash: String,
    author: String,
    shares: Vec<ShareMeta>,
}

#[derive(Clone, PartialEq, Deserialize)]
struct ShareMeta {
    pubkey: String,
    did: String,
    #[serde(default)]
    via: Option<String>,
}

/// Read the open vault's manifest metadata (synchronous — no network).
fn read_meta(fs: &FsHandle) -> Option<VaultMeta> {
    let guard = fs.borrow();
    let f = guard.as_ref()?;
    let json = f.manifest_info().ok()?;
    serde_json::from_str::<VaultMeta>(&json).ok()
}

fn guess_mime(name: &str) -> String {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "flac" => "audio/flac",
        "json" => "application/json",
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "html" | "htm" => "text/html",
        "csv" => "text/csv",
        "xml" => "application/xml",
        "txt" | "md" | "markdown" | "log" | "toml" | "yaml" | "yml" | "ini" | "conf" | "rs"
        | "py" | "go" | "c" | "h" | "cpp" | "sh" | "sql" => "text/plain",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn is_textual(mime: &str) -> bool {
    mime.starts_with("text/")
        || matches!(
            mime,
            "application/json" | "application/xml" | "application/javascript"
        )
}

/// Build an object URL over `bytes` tagged with `mime`, for blob-backed
/// rendering (`<img src>`, `<iframe src>`, download links).
fn object_url(bytes: &[u8], mime: &str) -> Result<String, String> {
    let arr = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&arr);
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime);
    let blob =
        web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts).map_err(jserr)?;
    web_sys::Url::create_object_url_with_blob(&blob).map_err(jserr)
}

#[derive(Properties, PartialEq)]
pub struct Props {
    pub vault_id: String,
}

/// Abbreviate a 64-char hex vault id for display: `01fc44f0\u{2026}405e38`.
/// The full id is in the Details panel.
fn short_id(id: &str) -> String {
    if id.len() > 20 {
        format!("{}\u{2026}{}", &id[..8], &id[id.len() - 6..])
    } else {
        id.to_string()
    }
}

async fn open(fs: &FsHandle, vault_id: String) -> Result<(), String> {
    let opened = WasmFs::open(vault_id, origin())
        .await
        .map_err(|e| jserr(e.into()))?;
    *fs.borrow_mut() = Some(opened);
    Ok(())
}

async fn ls(fs: &FsHandle, path: String) -> Result<Vec<FsEntry>, String> {
    let guard = fs.borrow();
    let f = guard.as_ref().ok_or("vault not open")?;
    let json = f.ls(path).await.map_err(|e| jserr(e.into()))?;
    serde_json::from_str::<Vec<FsEntry>>(&json).map_err(|e| e.to_string())
}

async fn cat(fs: &FsHandle, path: String) -> Result<Vec<u8>, String> {
    let guard = fs.borrow();
    let f = guard.as_ref().ok_or("vault not open")?;
    f.cat(path).await.map_err(|e| jserr(e.into()))
}

/// Sync the open vault to the hub's head if another device advanced it.
/// Best-effort: a failed refresh (offline, transient) keeps the current
/// view rather than breaking navigation.
async fn refresh(fs: &FsHandle) {
    let mut guard = fs.borrow_mut();
    if let Some(f) = guard.as_mut() {
        let _ = f.refresh().await;
    }
}

/// Run `op` + save; on a head conflict (another device wrote since we
/// loaded — the hub answers 409), fast-forward and replay once.
async fn save_with_retry<F>(fs: &FsHandle, op: F) -> Result<(), String>
where
    F: for<'a> AsyncFn(&'a mut WasmFs) -> Result<(), String>,
{
    let mut guard = fs.borrow_mut();
    let f = guard.as_mut().ok_or("vault not open")?;
    op(f).await?;
    match f.save().await {
        Ok(_) => Ok(()),
        Err(e) => {
            let msg = jserr(e.into());
            if !msg.contains("409") {
                return Err(msg);
            }
            f.refresh().await.map_err(|e| jserr(e.into()))?;
            op(f).await?;
            f.save().await.map_err(|e| jserr(e.into()))?;
            Ok(())
        }
    }
}

async fn add_and_save(fs: &FsHandle, path: String, bytes: Vec<u8>) -> Result<(), String> {
    save_with_retry(fs, async |f: &mut WasmFs| {
        f.add_file(path.clone(), bytes.clone())
            .await
            .map_err(|e| jserr(e.into()))
    })
    .await
}

async fn mkdir_and_save(fs: &FsHandle, path: String) -> Result<(), String> {
    save_with_retry(fs, async |f: &mut WasmFs| {
        f.mkdir(path.clone()).await.map_err(|e| jserr(e.into()))
    })
    .await
}

async fn rm_and_save(fs: &FsHandle, path: String) -> Result<(), String> {
    save_with_retry(fs, async |f: &mut WasmFs| {
        f.rm(path.clone()).await.map_err(|e| jserr(e.into()))
    })
    .await
}

/// Load `path` into an [`OpenFile`] (decrypt, object-URL, decode text).
async fn load_file(fs: &FsHandle, path: String) -> Result<OpenFile, String> {
    let bytes = cat(fs, path.clone()).await?;
    let name = path.rsplit('/').next().unwrap_or_default();
    let mime = guess_mime(name);
    let url = object_url(&bytes, &mime)?;
    let text = if is_textual(&mime) && bytes.len() < 2_000_000 {
        String::from_utf8(bytes).ok()
    } else {
        None
    };
    Ok(OpenFile {
        path,
        mime,
        url,
        text,
    })
}


/// Join `dir` + `name` into a vault path.
fn joined(dir: &str, name: &str) -> String {
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
}

#[function_component(VaultTree)]
pub fn vault_tree(props: &Props) -> Html {
    let fs: FsHandle = use_mut_ref(|| None::<WasmFs>);
    let expanded = use_state(BTreeSet::<String>::new);
    let rows = use_state(|| None::<Vec<TreeNode>>);
    let open_file = use_state(|| None::<OpenFile>);
    let error = use_state(String::new);
    let status = use_state(String::new);
    // The in-flight edit: Some(text) once the open file's content has
    // been touched; cleared on save / file switch. Drives the header's
    // Save affordance.
    let draft = use_state(|| None::<String>);
    let meta = use_state(|| None::<VaultMeta>);
    let devices = use_state(Vec::<Device>::new);
    let web_did = use_state(String::new);
    let show_details = use_state(|| false);
    let vaults = use_state(Vec::<VaultItem>::new);
    let ctx = use_state(|| None::<CtxMenu>);
    let upload_dir = use_state(|| "/".to_string());
    let upload_input = use_node_ref();

    // Rebuild the whole visible tree: refresh from the hub, walk root +
    // every expanded dir, refresh manifest metadata. Expansion state is
    // preserved, so this is the one refresh path after any mutation.
    let rebuild = {
        let fs = fs.clone();
        let expanded = expanded.clone();
        let rows = rows.clone();
        let error = error.clone();
        let meta = meta.clone();
        Callback::from(move |_: ()| {
            let fs = fs.clone();
            let expanded_set = (*expanded).clone();
            let rows = rows.clone();
            let error = error.clone();
            let meta = meta.clone();
            yew::platform::spawn_local(async move {
                if fs.borrow().is_none() {
                    return; // pre-open effect fire; mount path rebuilds explicitly
                }
                refresh(&fs).await;
                let lister = {
                    let fs = fs.clone();
                    move |p: String| {
                        let fs = fs.clone();
                        async move { ls(&fs, p).await }
                    }
                };
                match build_rows(lister, &expanded_set).await {
                    Ok(r) => rows.set(Some(r)),
                    Err(e) => error.set(e),
                }
                meta.set(read_meta(&fs));
            });
        })
    };

    // Open on mount, then build the root tree; load the account's vault
    // list (switcher) + devices (details panel).
    {
        let fs = fs.clone();
        let rebuild = rebuild.clone();
        let error = error.clone();
        let devices = devices.clone();
        let web_did = web_did.clone();
        let vaults = vaults.clone();
        let vault_id = props.vault_id.clone();
        use_effect_with(vault_id.clone(), move |_| {
            let fs = fs.clone();
            let rebuild = rebuild.clone();
            let error = error.clone();
            let devices = devices.clone();
            let web_did = web_did.clone();
            let vaults = vaults.clone();
            yew::platform::spawn_local(async move {
                match open(&fs, vault_id).await {
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

    // Expansion set drives the tree; rebuild on every change.
    {
        let rebuild = rebuild.clone();
        let expanded_now = (*expanded).clone();
        use_effect_with(expanded_now, move |_| {
            rebuild.emit(());
            || ()
        });
    }

    // Revoke the open file's object URL when it changes or we unmount.
    {
        let url = (*open_file).as_ref().map(|p| p.url.clone());
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

    let on_open = {
        let fs = fs.clone();
        let open_file = open_file.clone();
        let error = error.clone();
        Callback::from(move |path: String| {
            let fs = fs.clone();
            let open_file = open_file.clone();
            let error = error.clone();
            yew::platform::spawn_local(async move {
                match load_file(&fs, path).await {
                    Ok(f) => {
                        error.set(String::new());
                        open_file.set(Some(f));
                    }
                    Err(e) => error.set(e),
                }
            });
        })
    };
    // Switching files abandons the draft.
    {
        let draft = draft.clone();
        let status = status.clone();
        let path_now = (*open_file).as_ref().map(|f| f.path.clone());
        use_effect_with(path_now, move |_| {
            draft.set(None);
            status.set(String::new());
            || ()
        });
    }

    let save_draft = {
        let fs = fs.clone();
        let open_file = open_file.clone();
        let draft = draft.clone();
        let error = error.clone();
        let status = status.clone();
        let rebuild = rebuild.clone();
        Callback::from(move |_: MouseEvent| {
            let Some(path) = (*open_file).as_ref().map(|f| f.path.clone()) else {
                return;
            };
            let Some(text) = (*draft).clone() else {
                return;
            };
            let fs = fs.clone();
            let open_file = open_file.clone();
            let draft = draft.clone();
            let error = error.clone();
            let status = status.clone();
            let rebuild = rebuild.clone();
            status.set("saving\u{2026}".to_string());
            yew::platform::spawn_local(async move {
                match add_and_save(&fs, path.clone(), text.into_bytes()).await {
                    Ok(()) => {
                        status.set("saved".to_string());
                        draft.set(None);
                        if let Ok(f) = load_file(&fs, path).await {
                            open_file.set(Some(f));
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

    let on_change = {
        let draft = draft.clone();
        let status = status.clone();
        Callback::from(move |text: String| {
            draft.set(Some(text));
            status.set(String::new());
        })
    };

    // --- creation / destruction (ctx menu + docked New) ---

    let new_note_in = {
        let fs = fs.clone();
        let open_file = open_file.clone();
        let error = error.clone();
        let status = status.clone();
        let rebuild = rebuild.clone();
        Callback::from(move |dir: String| {
            let name = web_sys::window()
                .and_then(|w| w.prompt_with_message("Note name:").ok().flatten())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let Some(mut name) = name else {
                return;
            };
            if !name.to_ascii_lowercase().ends_with(".md") {
                name.push_str(".md");
            }
            let fs = fs.clone();
            let path = joined(&dir, &name);
            let open_file = open_file.clone();
            let error = error.clone();
            let status = status.clone();
            let rebuild = rebuild.clone();
            status.set(format!("creating {name}\u{2026}"));
            yew::platform::spawn_local(async move {
                match add_and_save(&fs, path.clone(), Vec::new()).await {
                    Ok(()) => {
                        status.set(String::new());
                        if let Ok(f) = load_file(&fs, path).await {
                            open_file.set(Some(f));
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
        let fs = fs.clone();
        let error = error.clone();
        let status = status.clone();
        let rebuild = rebuild.clone();
        Callback::from(move |dir: String| {
            let name = web_sys::window()
                .and_then(|w| w.prompt_with_message("Folder name:").ok().flatten())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let Some(name) = name else {
                return;
            };
            let fs = fs.clone();
            let path = joined(&dir, &name);
            let error = error.clone();
            let status = status.clone();
            let rebuild = rebuild.clone();
            status.set(format!("creating {name}\u{2026}"));
            yew::platform::spawn_local(async move {
                match mkdir_and_save(&fs, path).await {
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

    let remove_path = {
        let fs = fs.clone();
        let open_file = open_file.clone();
        let error = error.clone();
        let status = status.clone();
        let rebuild = rebuild.clone();
        Callback::from(move |path: String| {
            let name = path.rsplit('/').next().unwrap_or_default().to_string();
            let ok = web_sys::window()
                .and_then(|w| w.confirm_with_message(&format!("Delete {name}?")).ok())
                .unwrap_or(false);
            if !ok {
                return;
            }
            let fs = fs.clone();
            let open_file = open_file.clone();
            let error = error.clone();
            let status = status.clone();
            let rebuild = rebuild.clone();
            status.set(format!("deleting {name}\u{2026}"));
            yew::platform::spawn_local(async move {
                match rm_and_save(&fs, path.clone()).await {
                    Ok(()) => {
                        status.set(String::new());
                        if (*open_file).as_ref().is_some_and(|f| f.path == path) {
                            open_file.set(None);
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

    let upload_files = {
        let fs = fs.clone();
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
            let fs = fs.clone();
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
                    if let Err(e) = add_and_save(&fs, joined(&dir, &name), bytes).await {
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
        let mk = |cb: Callback<String>, arg: String, ctx: &UseStateHandle<Option<CtxMenu>>| {
            let ctx = ctx.clone();
            Callback::from(move |_: MouseEvent| {
                ctx.set(None);
                cb.emit(arg.clone());
            })
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
                    { item("New note", mk(new_note_in.clone(), dir.clone(), &ctx)) }
                    { item("New folder", mk(new_folder_in.clone(), dir.clone(), &ctx)) }
                    { item("Upload here", trigger_upload) }
                    {
                        match &m.target {
                            CtxTarget::File(p) | CtxTarget::Folder(p) => {
                                html! { { item("Delete", mk(remove_path.clone(), p.clone(), &ctx)) } }
                            }
                            CtxTarget::Root => html! {},
                        }
                    }
                </div>
            </>
        }
    });

    let vault_title = match &*meta {
        Some(m) if !m.name.is_empty() => m.name.clone(),
        _ => short_id(&props.vault_id),
    };
    let active = (*open_file).as_ref().map(|f| f.path.clone());

    // Breadcrumb: [vault switcher] / file / path / segments
    let crumbs = {
        let mut items: Vec<Html> = Vec::new();
        items.push(html! {
            <HeaderMenu class={classes!("vault-switch")}
                trigger_class={classes!("app-header__crumb", "app-header__crumb--current", "vault-switch__trigger")}
                trigger={html! { <>{ vault_title.clone() }{ " \u{25BE}" }</> }}>
                { for vaults.iter().map(|v| {
                    let name = if v.name.is_empty() { short_id(&v.vault_id) } else { v.name.clone() };
                    html! {
                        <Link<Route> to={Route::Vault { id: v.vault_id.clone() }} classes="app-menu__item">
                            { name }
                        </Link<Route>>
                    }
                }) }
                <Link<Route> to={Route::Workspace} classes="app-menu__item">{ "all vaults \u{2192}" }</Link<Route>>
            </HeaderMenu>
        });
        if let Some(f) = &*open_file {
            for seg in f.path.trim_start_matches('/').split('/') {
                items.push(html! { <span class="app-header__sep">{ "/" }</span> });
                items.push(html! { <span class="app-header__crumb app-header__crumb--current">{ seg.to_string() }</span> });
            }
        }
        items
    };

    let toggle_details = {
        let show_details = show_details.clone();
        Callback::from(move |_: MouseEvent| show_details.set(!*show_details))
    };

    html! {
        <>
            <header class="app-header">
                <HeaderMenu trigger={html! { "\u{2630}" }}>
                    <Link<Route> to={Route::Workspace} classes="app-menu__item">{ "Workspace" }</Link<Route>>
                    <Link<Route> to={Route::Settings} classes="app-menu__item">{ "Settings" }</Link<Route>>
                    <a href="/auth/logout" class="app-menu__item">{ "Sign out" }</a>
                </HeaderMenu>
                <div class="app-header__slot app-header__slot--left">{ crumbs }</div>
                <div class="app-header__slot app-header__slot--right">
                    <span id="save-status" class="app-header__status">{ (*status).clone() }</span>
                    if draft.is_some() {
                        <button type="button" class="app-header__btn app-header__btn--save" onclick={save_draft.clone()}>
                            { "Save" }
                        </button>
                    }
                    <button type="button" class="app-header__btn" title="Vault details" onclick={toggle_details}>
                        { "\u{24D8}" }
                    </button>
                </div>
            </header>

            <div class="editor-layout">
                {
                    match &*rows {
                        None => html! {
                            <aside class="tree-pane"><div class="doc-empty">{ "Loading\u{2026}" }</div></aside>
                        },
                        Some(r) => html! {
                            <TreePane rows={r.clone()} active={active.clone()}
                                on_toggle={on_toggle} on_open={on_open.clone()}
                                on_new_note={
                                    let new_note_in = new_note_in.clone();
                                    Callback::from(move |_: ()| new_note_in.emit("/".to_string()))
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
                                on_ctx={on_ctx} />
                        },
                    }
                }
                <div class="editor-main">
                    <div class="editor-body">
                        if !(*error).is_empty() {
                            <div class="error" style="margin:1rem 2rem 0;">{ (*error).clone() }</div>
                        }
                        if *show_details {
                            <div class="editor-container" style="padding-bottom:0;">
                                {
                                    match &*meta {
                                        None => html! { <div class="vault-details muted">{ "Loading\u{2026}" }</div> },
                                        Some(m) => details_panel(m, &devices, &web_did),
                                    }
                                }
                            </div>
                        }
                        {
                            match &*open_file {
                                Some(f) => html! {
                                    <EditorPane file={f.clone()} on_change={on_change.clone()} />
                                },
                                None => html! {
                                    <div class="doc-empty">
                                        { "Select a file, or right-click the tree for more." }
                                    </div>
                                },
                            }
                        }
                    </div>
                </div>
            </div>

            <input type="file" multiple={true} style="display:none;"
                ref={upload_input} onchange={upload_files} />

            { ctx_menu_html.unwrap_or_default() }
        </>
    }
}

/// The Details panel: vault metadata + shareholders, flagging which keys
/// are the signed-in user's own enrolled devices.
fn details_panel(meta: &VaultMeta, devices: &[Device], web_did: &str) -> Html {
    html! {
        <div class="card vault-details" style="padding:1rem;">
            <div style="display:grid;grid-template-columns:auto 1fr;gap:0.35rem 1rem;align-items:baseline;">
                <span class="muted">{ "name" }</span>
                <span>{ meta.name.clone() }</span>
                <span class="muted">{ "height" }</span>
                <span>{ meta.height }</span>
                <span class="muted">{ "vault id" }</span>
                <code style="word-break:break-all;font-size:0.72rem;">{ meta.vault_id.clone() }</code>
                <span class="muted">{ "manifest" }</span>
                <code style="word-break:break-all;font-size:0.72rem;">{ meta.manifest_hash.clone() }</code>
            </div>
            <h3 style="font-size:0.9rem;margin:1rem 0 0.5rem;">
                { format!("Shareholders ({})", meta.shares.len()) }
            </h3>
            <div style="display:flex;flex-direction:column;gap:0.4rem;">
                { for meta.shares.iter().map(|s| share_row(s, devices, web_did, &meta.author)) }
            </div>
        </div>
    }
}

fn share_row(s: &ShareMeta, devices: &[Device], web_did: &str, author: &str) -> Html {
    let mine = devices.iter().find(|d| d.pubkey == s.pubkey);
    let is_author = s.pubkey == author;
    // The web key's proper identity is the account `did:web` (reached via the
    // hub), not the `did:key` sealed into the manifest. Only swap when we can
    // confirm this share *is* our web key (its pubkey matches the web device).
    let is_web_key = mine.is_some_and(|d| d.kind == "web");
    let did = if is_web_key && !web_did.is_empty() {
        web_did.to_string()
    } else {
        s.did.clone()
    };
    html! {
        <div style="display:flex;align-items:center;gap:0.5rem;padding:0.5rem 0.6rem;border:1px solid hsl(var(--border));border-radius:var(--radius);">
            <code style="font-size:0.72rem;word-break:break-all;">{ did }</code>
            <span style="flex:1;"></span>
            if is_author {
                <span class="device-row__badge">{ "author" }</span>
            }
            if let Some(d) = mine {
                <span class="device-row__badge device-row__badge--master">{ own_label(d) }</span>
            } else {
                <span class="device-row__badge">{ "external" }</span>
            }
        </div>
    }
}

fn own_label(d: &Device) -> String {
    match &d.label {
        Some(l) if !l.is_empty() => l.clone(),
        _ if d.kind == "web" => "your web key".to_string(),
        _ => "your daemon".to_string(),
    }
}
