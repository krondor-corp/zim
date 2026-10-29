//! Desktop transport: the same [`VaultBackend`] seam, but every call goes
//! through Tauri's `invoke` to the shell, which proxies the local zim
//! daemon's HTTP API. No WASM crypto here — the daemon owns the keys and
//! the blobs; the SPA only ever sees plaintext paths and bytes.
//!
//! Mode is detected at runtime ([`is_desktop`]): Tauri injects
//! `window.__TAURI_INTERNALS__` before any page script runs. One Trunk
//! build then serves both the hub (browser) and the desktop shell, with
//! no cargo feature to keep in sync — the cost is that the desktop code
//! is compiled into the browser bundle, a few hundred lines of glue.
//!
//! File bytes cross as `Uint8Array` / `ArrayBuffer` (Tauri's raw IPC
//! payloads), never JSON number arrays.

use js_sys::{Function, Reflect, Uint8Array};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::{ReadError, VaultBackend};
use crate::api::{FsEntry, VaultItem};

/// Are we running inside the Tauri shell?
pub fn is_desktop() -> bool {
    internals().is_some()
}

fn internals() -> Option<JsValue> {
    let win = web_sys::window()?;
    Reflect::get(&win, &"__TAURI_INTERNALS__".into())
        .ok()
        .filter(|v| !v.is_undefined() && !v.is_null())
}

/// `invoke(cmd, args, options)` → the resolved JS value, or the
/// command's error string.
pub(crate) async fn invoke_raw(
    cmd: &str,
    args: JsValue,
    options: JsValue,
) -> Result<JsValue, String> {
    let internals = internals().ok_or("not running inside the desktop app")?;
    let invoke: Function = Reflect::get(&internals, &"invoke".into())
        .map_err(|_| "tauri invoke missing")?
        .dyn_into()
        .map_err(|_| "tauri invoke is not a function")?;
    let promise = invoke
        .call3(&internals, &cmd.into(), &args, &options)
        .map_err(|e| e.as_string().unwrap_or_else(|| "invoke failed".into()))?;
    JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|e| {
            e.as_string()
                .or_else(|| js_sys::Error::from(e).message().as_string())
                .unwrap_or_else(|| "command failed".into())
        })
}

fn js(value: serde_json::Value) -> JsValue {
    js_sys::JSON::parse(&value.to_string()).unwrap_or(JsValue::UNDEFINED)
}

/// Invoke a command with JSON args and decode its JSON reply.
pub(crate) async fn invoke<T: DeserializeOwned>(
    cmd: &str,
    args: serde_json::Value,
) -> Result<T, String> {
    let out = invoke_raw(cmd, js(args), JsValue::UNDEFINED).await?;
    let json = js_sys::JSON::stringify(&out)
        .ok()
        .and_then(|s| s.as_string())
        .ok_or("unreadable reply")?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

fn encode(s: &str) -> String {
    String::from(js_sys::encode_uri_component(s))
}

#[derive(Deserialize)]
struct Head {
    height: u64,
}

#[derive(Deserialize)]
struct Entry {
    name: String,
    kind: String,
}

/// The vault the desktop backend has open, and the head height it last saw.
#[derive(Clone)]
pub(super) struct Opened {
    pub vault_id: String,
    pub height: u64,
}

pub async fn list_vaults() -> Result<Vec<VaultItem>, String> {
    invoke("vault_list", serde_json::json!({})).await
}

pub async fn create_vault(name: String) -> Result<String, String> {
    let v: VaultItem = invoke("vault_create", serde_json::json!({ "name": name })).await?;
    Ok(v.vault_id)
}

impl VaultBackend {
    fn opened(&self) -> Result<Opened, String> {
        self.desktop
            .borrow()
            .clone()
            .ok_or_else(|| "vault not open".to_string())
    }

    pub(super) async fn desktop_open(&self, vault_id: String) -> Result<(), String> {
        let head: Head = invoke("vault_head", serde_json::json!({ "vaultId": vault_id })).await?;
        *self.desktop.borrow_mut() = Some(Opened {
            vault_id,
            height: head.height,
        });
        Ok(())
    }

    pub(super) async fn desktop_ls(&self, path: String) -> Result<Vec<FsEntry>, String> {
        let o = self.opened()?;
        let items: Vec<Entry> = invoke(
            "vault_ls",
            serde_json::json!({ "vaultId": o.vault_id, "path": path }),
        )
        .await?;
        Ok(items
            .into_iter()
            .map(|e| FsEntry {
                name: e.name,
                kind: e.kind,
                hash: String::new(),
                mime: None,
            })
            .collect())
    }

    pub(super) async fn desktop_cat(&self, path: String) -> Result<Vec<u8>, ReadError> {
        let o = self.opened().map_err(ReadError::Failed)?;
        let out = invoke_raw(
            "vault_cat",
            js(serde_json::json!({ "vaultId": o.vault_id, "path": path })),
            JsValue::UNDEFINED,
        )
        .await
        .map_err(ReadError::Failed)?;
        // `tauri::ipc::Response` arrives as an ArrayBuffer.
        Ok(Uint8Array::new(&out).to_vec())
    }

    /// Follow the daemon: `true` when its head moved since we last looked
    /// (a peer synced a change in). Best-effort, like the hub's refresh.
    pub(super) async fn desktop_refresh(&self) -> bool {
        let Ok(o) = self.opened() else { return false };
        let Ok(head) =
            invoke::<Head>("vault_head", serde_json::json!({ "vaultId": o.vault_id })).await
        else {
            return false;
        };
        let moved = head.height != o.height;
        self.set_height(head.height);
        moved
    }

    pub(super) async fn desktop_write(&self, path: String, bytes: Vec<u8>) -> Result<(), String> {
        let o = self.opened()?;
        let headers = js_sys::Object::new();
        let _ = Reflect::set(&headers, &"x-zim-vault".into(), &o.vault_id.as_str().into());
        let _ = Reflect::set(&headers, &"x-zim-path".into(), &encode(&path).into());
        let options = js_sys::Object::new();
        let _ = Reflect::set(&options, &"headers".into(), &headers);
        let body = Uint8Array::from(bytes.as_slice());
        let height = invoke_raw("vault_add", body.into(), options.into()).await?;
        self.note_height(height);
        Ok(())
    }

    pub(super) async fn desktop_mutate(
        &self,
        cmd: &str,
        args: serde_json::Value,
    ) -> Result<(), String> {
        let height = invoke_raw(cmd, js(args), JsValue::UNDEFINED).await?;
        self.note_height(height);
        Ok(())
    }

    pub(super) fn vault_arg(&self) -> Result<String, String> {
        Ok(self.opened()?.vault_id)
    }

    /// Our own write advanced the head — don't report it back to the
    /// follow loop as a remote change.
    fn note_height(&self, height: JsValue) {
        if let Some(h) = height.as_f64() {
            self.set_height(h as u64);
        }
    }

    fn set_height(&self, height: u64) {
        if let Some(o) = self.desktop.borrow_mut().as_mut() {
            o.height = height;
        }
    }
}
