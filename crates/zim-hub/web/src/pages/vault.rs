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

use crate::api::{fetch_devices, fetch_me, Device, FsEntry};
use crate::pages::vault_editor::{EditorPane, OpenFile};
use crate::pages::vault_tree::{build_rows, TreeNode, TreePane};
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

#[function_component(VaultTree)]
pub fn vault_tree(props: &Props) -> Html {
    let fs: FsHandle = use_mut_ref(|| None::<WasmFs>);
    let expanded = use_state(BTreeSet::<String>::new);
    let rows = use_state(|| None::<Vec<TreeNode>>);
    let open_file = use_state(|| None::<OpenFile>);
    let error = use_state(String::new);
    let status = use_state(String::new);
    let meta = use_state(|| None::<VaultMeta>);
    let devices = use_state(Vec::<Device>::new);
    // The account's `did:web` — shown for the web key in the shareholders list
    // instead of its raw `did:key`. Empty until `/me` resolves.
    let web_did = use_state(String::new);
    let show_details = use_state(|| false);

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

    // Open on mount, then build the root tree.
    {
        let fs = fs.clone();
        let rebuild = rebuild.clone();
        let error = error.clone();
        let devices = devices.clone();
        let web_did = web_did.clone();
        let vault_id = props.vault_id.clone();
        use_effect_with(vault_id.clone(), move |_| {
            let fs = fs.clone();
            let rebuild = rebuild.clone();
            let error = error.clone();
            let devices = devices.clone();
            let web_did = web_did.clone();
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
            });
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

    // Toggling only mutates the expansion set; the effect below owns
    // the rebuild so it always sees the freshly-set value.
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

    {
        let rebuild = rebuild.clone();
        let expanded_now = (*expanded).clone();
        use_effect_with(expanded_now, move |_| {
            rebuild.emit(());
            || ()
        });
    }

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

    let on_save = {
        let fs = fs.clone();
        let open_file = open_file.clone();
        let error = error.clone();
        let status = status.clone();
        let rebuild = rebuild.clone();
        Callback::from(move |(path, text): (String, String)| {
            let fs = fs.clone();
            let open_file = open_file.clone();
            let error = error.clone();
            let status = status.clone();
            let rebuild = rebuild.clone();
            status.set("saving\u{2026}".to_string());
            yew::platform::spawn_local(async move {
                match add_and_save(&fs, path.clone(), text.into_bytes()).await {
                    Ok(()) => {
                        status.set(String::new());
                        // Reload so the buffer/url reflect the committed state.
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

    let on_close = {
        let open_file = open_file.clone();
        Callback::from(move |_: ()| open_file.set(None))
    };

    let on_remove = {
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

    let new_note = {
        let fs = fs.clone();
        let open_file = open_file.clone();
        let error = error.clone();
        let status = status.clone();
        let rebuild = rebuild.clone();
        Callback::from(move |_: MouseEvent| {
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
            let title = name.trim_end_matches(".md").to_string();
            let fs = fs.clone();
            let path = format!("/{name}");
            let open_file = open_file.clone();
            let error = error.clone();
            let status = status.clone();
            let rebuild = rebuild.clone();
            status.set(format!("creating {name}\u{2026}"));
            yew::platform::spawn_local(async move {
                let body = format!("# {title}\n\n");
                match add_and_save(&fs, path.clone(), body.into_bytes()).await {
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

    let new_folder = {
        let fs = fs.clone();
        let error = error.clone();
        let status = status.clone();
        let rebuild = rebuild.clone();
        Callback::from(move |_: MouseEvent| {
            let name = web_sys::window()
                .and_then(|w| w.prompt_with_message("Folder name:").ok().flatten())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let Some(name) = name else {
                return;
            };
            let fs = fs.clone();
            let path = format!("/{name}");
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

    let upload = {
        let fs = fs.clone();
        let error = error.clone();
        let status = status.clone();
        let rebuild = rebuild.clone();
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
                    if let Err(e) = add_and_save(&fs, format!("/{name}"), bytes).await {
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

    let vault_title = match &*meta {
        Some(m) if !m.name.is_empty() => m.name.clone(),
        _ => short_id(&props.vault_id),
    };
    let active = (*open_file).as_ref().map(|f| f.path.clone());

    html! {
        <div class="vault-shell">
            <aside class="vault-side">
                <div class="vault-side__head">
                    <Link<Route> to={Route::Workspace} classes="vault-side__back">{ "\u{2190}" }</Link<Route>>
                    <span class="vault-side__title" title={props.vault_id.clone()}>{ vault_title }</span>
                    <button type="button" class="vault-side__details" title="Vault details" onclick={
                        let show_details = show_details.clone();
                        Callback::from(move |_: MouseEvent| show_details.set(!*show_details))
                    }>{ "\u{24D8}" }</button>
                </div>
                <div class="vault-side__actions">
                    <button type="button" class="btn" onclick={new_note}>{ "New note" }</button>
                    <button type="button" class="btn" onclick={new_folder}>{ "Folder" }</button>
                    <label class="btn" style="cursor:pointer;">
                        { "Upload" }
                        <input type="file" multiple={true} style="display:none;" onchange={upload} />
                    </label>
                </div>
                if !(*status).is_empty() {
                    <div class="vault-side__status muted">{ (*status).clone() }</div>
                }
                {
                    match &*rows {
                        None => html! { <div class="muted tree-empty">{ "Loading\u{2026}" }</div> },
                        Some(r) => html! {
                            <TreePane rows={r.clone()} active={active.clone()}
                                on_toggle={on_toggle} on_open={on_open.clone()} on_remove={on_remove} />
                        },
                    }
                }
            </aside>
            <section class="vault-main">
                if !(*error).is_empty() {
                    <div class="error">{ (*error).clone() }</div>
                }
                if *show_details {
                    {
                        match &*meta {
                            None => html! { <div class="card" style="padding:1rem;"><span class="muted">{ "Loading\u{2026}" }</span></div> },
                            Some(m) => details_panel(m, &devices, &web_did),
                        }
                    }
                }
                {
                    match &*open_file {
                        Some(f) => html! {
                            <EditorPane file={f.clone()} on_save={on_save} on_close={on_close}
                                status={(*status).clone()} />
                        },
                        None => html! {
                            <div class="vault-main__empty muted">
                                <p>{ "Select a file from the tree, or create a note." }</p>
                            </div>
                        },
                    }
                }
            </section>
        </div>
    }
}

/// The Details panel: vault metadata + shareholders, flagging which keys
/// are the signed-in user's own enrolled devices.
fn details_panel(meta: &VaultMeta, devices: &[Device], web_did: &str) -> Html {
    html! {
        <div class="card" style="padding:1rem;margin-bottom:1rem;font-size:0.85rem;">
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
