//! Vault tree browser: open a vault and walk it by path. Read (`ls`/`cat`)
//! and edit (`add_file`/`mkdir`/`rm` then `save`) go through the held
//! `WasmFs` — all blob crypto happens inside the SDK. The handle lives in a
//! `RefCell` across user actions (which are sequential, so the borrows don't
//! overlap).

use std::cell::RefCell;
use std::rc::Rc;

use serde::Deserialize;
use wasm_bindgen_futures::JsFuture;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use zim_wasm::WasmFs;

use yew_router::prelude::*;

use crate::api::{fetch_devices, fetch_me, Device, FsEntry};
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

/// A decrypted file ready to preview: its name, resolved MIME, an object-URL
/// over the bytes (for `<img>`/`<video>`/`<iframe download>` etc.), and the
/// decoded text when it's a textual type.
#[derive(Clone, PartialEq)]
struct Preview {
    name: String,
    mime: String,
    url: String,
    text: Option<String>,
}

/// MIME from the manifest entry, falling back to a guess from the extension
/// (files added in-browser may not carry a MIME).
fn resolve_mime(entry: &FsEntry) -> String {
    match entry.mime.as_deref() {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => guess_mime(&entry.name),
    }
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

fn path_of(stack: &[String]) -> String {
    if stack.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", stack.join("/"))
    }
}
fn child_of(stack: &[String], name: &str) -> String {
    if stack.is_empty() {
        format!("/{name}")
    } else {
        format!("/{}/{}", stack.join("/"), name)
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

async fn add_and_save(fs: &FsHandle, path: String, bytes: Vec<u8>) -> Result<(), String> {
    let mut guard = fs.borrow_mut();
    let f = guard.as_mut().ok_or("vault not open")?;
    f.add_file(path, bytes).await.map_err(|e| jserr(e.into()))?;
    f.save().await.map_err(|e| jserr(e.into()))?;
    Ok(())
}

async fn mkdir_and_save(fs: &FsHandle, path: String) -> Result<(), String> {
    let mut guard = fs.borrow_mut();
    let f = guard.as_mut().ok_or("vault not open")?;
    f.mkdir(path).await.map_err(|e| jserr(e.into()))?;
    f.save().await.map_err(|e| jserr(e.into()))?;
    Ok(())
}

async fn rm_and_save(fs: &FsHandle, path: String) -> Result<(), String> {
    let mut guard = fs.borrow_mut();
    let f = guard.as_mut().ok_or("vault not open")?;
    f.rm(path).await.map_err(|e| jserr(e.into()))?;
    f.save().await.map_err(|e| jserr(e.into()))?;
    Ok(())
}

#[function_component(VaultTree)]
pub fn vault_tree(props: &Props) -> Html {
    let fs: FsHandle = use_mut_ref(|| None::<WasmFs>);
    let stack = use_state(Vec::<String>::new);
    let entries = use_state(|| None::<Vec<FsEntry>>);
    let preview = use_state(|| None::<Preview>);
    let error = use_state(String::new);
    let status = use_state(String::new);
    let meta = use_state(|| None::<VaultMeta>);
    let devices = use_state(Vec::<Device>::new);
    // The account's `did:web` — shown for the web key in the shareholders list
    // instead of its raw `did:key`. Empty until `/me` resolves.
    let web_did = use_state(String::new);
    let show_details = use_state(|| false);

    // ls a path and render it; refresh the manifest metadata too (cheap,
    // synchronous read of the open vault — keeps height/shares current
    // after navigation and edits).
    let load = {
        let fs = fs.clone();
        let entries = entries.clone();
        let error = error.clone();
        let meta = meta.clone();
        Callback::from(move |path: String| {
            let fs = fs.clone();
            let entries = entries.clone();
            let error = error.clone();
            let meta = meta.clone();
            yew::platform::spawn_local(async move {
                match ls(&fs, path).await {
                    Ok(es) => entries.set(Some(es)),
                    Err(e) => error.set(e),
                }
                meta.set(read_meta(&fs));
            });
        })
    };

    // Open on mount, then list root.
    {
        let fs = fs.clone();
        let load = load.clone();
        let error = error.clone();
        let devices = devices.clone();
        let web_did = web_did.clone();
        let vault_id = props.vault_id.clone();
        use_effect_with(vault_id.clone(), move |_| {
            let fs = fs.clone();
            let load = load.clone();
            let error = error.clone();
            let devices = devices.clone();
            let web_did = web_did.clone();
            yew::platform::spawn_local(async move {
                match open(&fs, vault_id).await {
                    Ok(()) => load.emit("/".to_string()),
                    Err(e) => error.set(e),
                }
                // For flagging which shareholders are our own devices, and
                // rendering the web key by its account `did:web`.
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

    // Revoke the preview's object URL when it changes or the view unmounts.
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

    let enter_dir = {
        let stack = stack.clone();
        let load = load.clone();
        let preview = preview.clone();
        Callback::from(move |name: String| {
            let mut next = (*stack).clone();
            next.push(name);
            stack.set(next.clone());
            preview.set(None);
            load.emit(path_of(&next));
        })
    };

    let crumb = {
        let stack = stack.clone();
        let load = load.clone();
        let preview = preview.clone();
        Callback::from(move |depth: usize| {
            let next: Vec<String> = stack.iter().take(depth).cloned().collect();
            stack.set(next.clone());
            preview.set(None);
            load.emit(path_of(&next));
        })
    };

    let open_file = {
        let fs = fs.clone();
        let stack = stack.clone();
        let preview = preview.clone();
        let error = error.clone();
        Callback::from(move |(name, mime): (String, String)| {
            let fs = fs.clone();
            let path = child_of(&stack, &name);
            let preview = preview.clone();
            let error = error.clone();
            yew::platform::spawn_local(async move {
                match cat(&fs, path).await {
                    Ok(bytes) => match object_url(&bytes, &mime) {
                        Ok(url) => {
                            let text = if is_textual(&mime) && bytes.len() < 2_000_000 {
                                String::from_utf8(bytes).ok()
                            } else {
                                None
                            };
                            preview.set(Some(Preview {
                                name,
                                mime,
                                url,
                                text,
                            }));
                        }
                        Err(e) => error.set(e),
                    },
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let remove = {
        let fs = fs.clone();
        let stack = stack.clone();
        let load = load.clone();
        let error = error.clone();
        let status = status.clone();
        Callback::from(move |name: String| {
            let ok = web_sys::window()
                .and_then(|w| w.confirm_with_message(&format!("Delete {name}?")).ok())
                .unwrap_or(false);
            if !ok {
                return;
            }
            let fs = fs.clone();
            let path = child_of(&stack, &name);
            let here = path_of(&stack);
            let load = load.clone();
            let error = error.clone();
            let status = status.clone();
            status.set(format!("deleting {name}\u{2026}"));
            yew::platform::spawn_local(async move {
                match rm_and_save(&fs, path).await {
                    Ok(()) => {
                        status.set(String::new());
                        load.emit(here);
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
        let stack = stack.clone();
        let load = load.clone();
        let error = error.clone();
        let status = status.clone();
        Callback::from(move |_: MouseEvent| {
            let name = web_sys::window()
                .and_then(|w| w.prompt_with_message("Folder name:").ok().flatten())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let Some(name) = name else {
                return;
            };
            let fs = fs.clone();
            let path = child_of(&stack, &name);
            let here = path_of(&stack);
            let load = load.clone();
            let error = error.clone();
            let status = status.clone();
            status.set(format!("creating {name}\u{2026}"));
            yew::platform::spawn_local(async move {
                match mkdir_and_save(&fs, path).await {
                    Ok(()) => {
                        status.set(String::new());
                        load.emit(here);
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
        let stack = stack.clone();
        let load = load.clone();
        let error = error.clone();
        let status = status.clone();
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
            let cur = (*stack).clone();
            let here = path_of(&cur);
            let load = load.clone();
            let error = error.clone();
            let status = status.clone();
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
                    if let Err(e) = add_and_save(&fs, child_of(&cur, &name), bytes).await {
                        status.set(String::new());
                        error.set(e);
                        return;
                    }
                }
                status.set(String::new());
                load.emit(here);
            });
        })
    };

    let crumbs = {
        let mut items: Vec<Html> = Vec::new();
        let crumb0 = crumb.clone();
        items.push(html! {
            <a href="#" onclick={Callback::from(move |e: MouseEvent| { e.prevent_default(); crumb0.emit(0); })}>{ "/" }</a>
        });
        for (i, seg) in stack.iter().enumerate() {
            let crumb = crumb.clone();
            let depth = i + 1;
            items.push(html! { <span>{ " / " }</span> });
            items.push(html! {
                <a href="#" onclick={Callback::from(move |e: MouseEvent| { e.prevent_default(); crumb.emit(depth); })}>{ seg.clone() }</a>
            });
        }
        items
    };

    html! {
        <>
            <span class="page-eyebrow">
                <Link<Route> to={Route::Workspace}>{ "workspace" }</Link<Route>>{ " / vault" }
            </span>
            <h1 style="font-size:1.1rem;"><code style="font-size:0.85rem;">{ props.vault_id.clone() }</code></h1>

            <div style="margin-top:0.5rem;">
                <button type="button" class="btn" onclick={
                    let show_details = show_details.clone();
                    Callback::from(move |_: MouseEvent| show_details.set(!*show_details))
                } style="font-size:0.8rem;padding:0.15rem 0.6rem;">
                    { if *show_details { "Details \u{25B4}" } else { "Details \u{25BE}" } }
                </button>
            </div>
            if *show_details {
                {
                    match &*meta {
                        None => html! { <div class="card" style="padding:1rem;margin-top:0.5rem;"><span class="muted">{ "Loading\u{2026}" }</span></div> },
                        Some(m) => details_panel(m, &devices, &web_did),
                    }
                }
            }

            <div style="display:flex;gap:0.5rem;align-items:center;margin:1rem 0 0;">
                <label class="btn" style="cursor:pointer;">
                    { "Upload" }
                    <input type="file" multiple={true} style="display:none;" onchange={upload} />
                </label>
                <button type="button" class="btn" onclick={new_folder}>{ "New folder" }</button>
                if !(*status).is_empty() {
                    <span class="muted" style="font-size:0.8rem;">{ (*status).clone() }</span>
                }
            </div>

            <nav style="font-size:0.9rem;margin:1rem 0 0.5rem;">{ crumbs }</nav>

            if !(*error).is_empty() {
                <div class="error">{ (*error).clone() }</div>
            }

            <div class="card" style="padding:0;overflow:hidden;">
                {
                    match &*entries {
                        None => html! { <div class="muted" style="padding:0.75rem 1rem;">{ "Loading\u{2026}" }</div> },
                        Some(es) if es.is_empty() => html! { <div class="muted" style="padding:0.75rem 1rem;">{ "(empty directory)" }</div> },
                        Some(es) => html! { { for sorted(es).into_iter().map(|e| row(&e, &enter_dir, &open_file, &remove)) } },
                    }
                }
            </div>

            if let Some(p) = &*preview {
                <div style="margin-top:1rem;">
                    <div style="display:flex;align-items:center;gap:0.75rem;margin-bottom:0.5rem;">
                        <h3 style="margin:0;">{ p.name.clone() }</h3>
                        <span class="muted" style="font-size:0.75rem;">{ p.mime.clone() }</span>
                        <span style="flex:1;"></span>
                        <a class="btn" href={p.url.clone()} download={p.name.clone()}>{ "Download" }</a>
                        <button class="btn" onclick={
                            let preview = preview.clone();
                            Callback::from(move |_: MouseEvent| preview.set(None))
                        }>{ "Close" }</button>
                    </div>
                    { preview_body(p) }
                </div>
            }
        </>
    }
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

fn preview_body(p: &Preview) -> Html {
    let url = p.url.clone();
    if p.mime.starts_with("image/") {
        html! { <img src={url} alt={p.name.clone()} style="max-width:100%;max-height:70vh;border-radius:6px;" /> }
    } else if p.mime == "application/pdf" {
        html! { <iframe src={url} title={p.name.clone()} style="width:100%;height:70vh;border:1px solid hsl(var(--border));border-radius:6px;"></iframe> }
    } else if p.mime.starts_with("video/") {
        html! { <video src={url} controls={true} style="max-width:100%;max-height:70vh;border-radius:6px;" /> }
    } else if p.mime.starts_with("audio/") {
        html! { <audio src={url} controls={true} style="width:100%;" /> }
    } else if let Some(text) = &p.text {
        html! { <pre class="card" style="max-height:60vh;overflow:auto;margin:0;white-space:pre-wrap;word-break:break-word;font-size:0.8rem;padding:1rem;">{ text.clone() }</pre> }
    } else {
        html! { <p class="muted">{ format!("No inline preview for {} \u{2014} use Download.", p.mime) }</p> }
    }
}

fn row(
    e: &FsEntry,
    enter_dir: &Callback<String>,
    open_file: &Callback<(String, String)>,
    remove: &Callback<String>,
) -> Html {
    let is_dir = e.kind == "dir";
    let name = e.name.clone();
    let mime = resolve_mime(e);
    let onclick = {
        let name = name.clone();
        let mime = mime.clone();
        let enter_dir = enter_dir.clone();
        let open_file = open_file.clone();
        Callback::from(move |ev: MouseEvent| {
            ev.prevent_default();
            if is_dir {
                enter_dir.emit(name.clone());
            } else {
                open_file.emit((name.clone(), mime.clone()));
            }
        })
    };
    let on_del = {
        let name = name.clone();
        let remove = remove.clone();
        Callback::from(move |ev: MouseEvent| {
            ev.prevent_default();
            ev.stop_propagation();
            remove.emit(name.clone());
        })
    };
    let label = if is_dir {
        format!("{}/", e.name)
    } else {
        e.name.clone()
    };
    html! {
        <a href="#" {onclick}
           style="display:flex;gap:0.6rem;align-items:center;padding:0.55rem 1rem;text-decoration:none;color:inherit;border-bottom:1px solid hsl(var(--border));font-size:0.9rem;">
            <span>{ if is_dir { "\u{1F4C1}" } else { "\u{1F4C4}" } }</span>
            <span style="font-family:var(--font-mono);">{ label }</span>
            <span style="flex:1;"></span>
            if let Some(m) = &e.mime {
                <span class="muted" style="font-size:0.75rem;">{ m.clone() }</span>
            }
            <button type="button" class="btn" title="Delete" onclick={on_del}
                style="padding:0.1rem 0.45rem;font-size:0.75rem;">{ "\u{2715}" }</button>
        </a>
    }
}

/// The Details panel: vault metadata + shareholders, flagging which keys
/// are the signed-in user's own enrolled devices.
fn details_panel(meta: &VaultMeta, devices: &[Device], web_did: &str) -> Html {
    html! {
        <div class="card" style="padding:1rem;margin-top:0.5rem;font-size:0.85rem;">
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
