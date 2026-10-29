//! Desktop transport: the same [`VaultBackend`] seam, but every call goes
//! through Tauri's `invoke` to the shell, which proxies the local zim
//! daemon's HTTP API. No WASM crypto here — the daemon owns the keys and
//! the blobs; the SPA only ever sees plaintext paths and bytes.
//!
//! The contract is [`zim_api::desktop`]: arguments, results and errors
//! are the same Rust types the shell compiles, serialized by serde — no
//! hand-matched JSON on this side. Mode is detected at runtime
//! ([`is_desktop`]): Tauri injects `window.__TAURI_INTERNALS__` before any
//! page script runs, so one Trunk build serves both the hub (browser) and
//! the shell, with no cargo feature to keep in sync.
//!
//! File bytes cross as `Uint8Array` / `ArrayBuffer` (Tauri's raw IPC
//! payloads), never JSON number arrays.

use js_sys::{Function, Reflect, Uint8Array};
use serde::de::DeserializeOwned;
use serde::Serialize;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use zim_api::desktop::{
    cmd, AddHeaders, CommandError, CreateVault, Head, VaultMove, VaultPath, VaultRef, VaultSummary,
};
use zim_core::fs::{AbsPath, DirEntry};
use zim_core::vault::VaultId;

use super::{ReadError, VaultBackend};

type Result<T> = std::result::Result<T, CommandError>;

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

/// `invoke(cmd, args, options)` → the resolved JS value. A rejection
/// carries the shell's serialized [`CommandError`].
async fn invoke_raw(cmd: &str, args: JsValue, options: JsValue) -> Result<JsValue> {
    let ipc = |m: &str| CommandError::Ipc(m.to_string());
    let internals = internals().ok_or_else(|| ipc("not running inside the desktop app"))?;
    let invoke: Function = Reflect::get(&internals, &"invoke".into())
        .map_err(|_| ipc("tauri invoke missing"))?
        .dyn_into()
        .map_err(|_| ipc("tauri invoke is not a function"))?;
    let promise = invoke
        .call3(&internals, &cmd.into(), &args, &options)
        .map_err(|_| ipc("invoke failed"))?;
    JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|e| from_js(&e))
}

/// Decode a rejection: the shell's typed error, else its text.
fn from_js(e: &JsValue) -> CommandError {
    let json = js_sys::JSON::stringify(e).ok().and_then(|s| s.as_string());
    json.and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_else(|| {
            CommandError::Ipc(
                e.as_string()
                    .unwrap_or_else(|| "command failed".to_string()),
            )
        })
}

/// A serde value as a JS value.
fn to_js<T: Serialize>(value: &T) -> Result<JsValue> {
    let json = serde_json::to_string(value).map_err(|e| CommandError::Ipc(e.to_string()))?;
    js_sys::JSON::parse(&json).map_err(|_| CommandError::Ipc("unencodable arguments".into()))
}

fn from_value<T: DeserializeOwned>(v: &JsValue) -> Result<T> {
    let json = js_sys::JSON::stringify(v)
        .ok()
        .and_then(|s| s.as_string())
        .ok_or_else(|| CommandError::Ipc("unreadable reply".into()))?;
    serde_json::from_str(&json).map_err(|e| CommandError::Ipc(e.to_string()))
}

/// Invoke a command with one typed argument, decode its typed result.
pub(crate) async fn call<A: Serialize, T: DeserializeOwned>(cmd: &str, args: &A) -> Result<T> {
    let out = invoke_raw(cmd, to_js(&Args { args })?, JsValue::UNDEFINED).await?;
    from_value(&out)
}

/// Invoke a command that takes no argument.
pub(crate) async fn call0<T: DeserializeOwned>(cmd: &str) -> Result<T> {
    from_value(&invoke_raw(cmd, JsValue::UNDEFINED, JsValue::UNDEFINED).await?)
}

fn abs(path: &str) -> Result<AbsPath> {
    AbsPath::new(path).ok_or_else(|| CommandError::BadRequest(format!("invalid path: {path}")))
}

pub async fn list_vaults() -> std::result::Result<Vec<VaultSummary>, String> {
    call0(cmd::VAULT_LIST).await.map_err(|e| e.to_string())
}

pub async fn create_vault(name: String) -> std::result::Result<VaultId, String> {
    let v: VaultSummary = call(cmd::VAULT_CREATE, &CreateVault { name })
        .await
        .map_err(|e| e.to_string())?;
    Ok(v.id)
}

/// The vault the desktop backend has open, and the head height it last saw.
#[derive(Clone)]
pub(super) struct Opened {
    pub id: VaultId,
    pub height: u64,
}

impl VaultBackend {
    fn opened(&self) -> Result<Opened> {
        self.desktop
            .borrow()
            .clone()
            .ok_or_else(|| CommandError::BadRequest("vault not open".into()))
    }

    fn at(&self, path: &str) -> Result<VaultPath> {
        Ok(VaultPath {
            vault: self.opened()?.id,
            path: abs(path)?,
        })
    }

    pub(super) async fn desktop_open(&self, id: VaultId) -> std::result::Result<(), String> {
        let head: Head = call(cmd::VAULT_HEAD, &VaultRef { vault: id })
            .await
            .map_err(|e| e.to_string())?;
        *self.desktop.borrow_mut() = Some(Opened {
            id,
            height: head.height,
        });
        Ok(())
    }

    pub(super) async fn desktop_ls(
        &self,
        path: &str,
    ) -> std::result::Result<Vec<DirEntry>, String> {
        let run = async {
            let mut entries: Vec<DirEntry> = call(cmd::VAULT_LS, &self.at(path)?).await?;
            // Directories first, then by name — as the hub backend lists.
            entries.sort_by(|a, b| (a.kind, &a.name).cmp(&(b.kind, &b.name)));
            Ok(entries)
        };
        run.await.map_err(|e: CommandError| e.to_string())
    }

    pub(super) async fn desktop_cat(&self, path: &str) -> std::result::Result<Vec<u8>, ReadError> {
        let run = async {
            let out = invoke_raw(
                cmd::VAULT_CAT,
                to_js(&Args {
                    args: &self.at(path)?,
                })?,
                JsValue::UNDEFINED,
            )
            .await?;
            // `tauri::ipc::Response` arrives as an ArrayBuffer.
            Ok(Uint8Array::new(&out).to_vec())
        };
        run.await
            .map_err(|e: CommandError| ReadError::Failed(e.to_string()))
    }

    /// Follow the daemon: `true` when its head moved since we last looked
    /// (a peer synced a change in). Best-effort, like the hub's refresh.
    pub(super) async fn desktop_refresh(&self) -> bool {
        let Ok(o) = self.opened() else { return false };
        let Ok(head) = call::<_, Head>(cmd::VAULT_HEAD, &VaultRef { vault: o.id }).await else {
            return false;
        };
        self.set_height(head.height);
        head.height != o.height
    }

    pub(super) async fn desktop_write(
        &self,
        path: &str,
        bytes: Vec<u8>,
    ) -> std::result::Result<(), String> {
        let run = async {
            let target = AddHeaders {
                vault: self.opened()?.id,
                path: abs(path)?,
            };
            let headers = js_sys::Object::new();
            for (name, value) in target.to_pairs() {
                let _ = Reflect::set(&headers, &name.into(), &value.into());
            }
            let options = js_sys::Object::new();
            let _ = Reflect::set(&options, &"headers".into(), &headers);
            let body = Uint8Array::from(bytes.as_slice());
            let height = invoke_raw(cmd::VAULT_ADD, body.into(), options.into()).await?;
            self.note_height(from_value(&height)?);
            Ok(())
        };
        run.await.map_err(|e: CommandError| e.to_string())
    }

    pub(super) async fn desktop_mkdir(&self, path: &str) -> std::result::Result<(), String> {
        self.mutate(cmd::VAULT_MKDIR, self.at(path)).await
    }

    pub(super) async fn desktop_rm(&self, path: &str) -> std::result::Result<(), String> {
        self.mutate(cmd::VAULT_RM, self.at(path)).await
    }

    pub(super) async fn desktop_mv(&self, from: &str, to: &str) -> std::result::Result<(), String> {
        let args = self.opened().and_then(|o| {
            Ok(VaultMove {
                vault: o.id,
                from: abs(from)?,
                to: abs(to)?,
            })
        });
        self.mutate(cmd::VAULT_MV, args).await
    }

    /// Run a structure-changing command; it answers the new head height.
    async fn mutate<A: Serialize>(
        &self,
        command: &str,
        args: Result<A>,
    ) -> std::result::Result<(), String> {
        let run = async {
            let height: u64 = call(command, &args?).await?;
            self.note_height(height);
            Ok(())
        };
        run.await.map_err(|e: CommandError| e.to_string())
    }

    /// Our own write advanced the head — don't report it back to the
    /// follow loop as a remote change.
    fn note_height(&self, height: u64) {
        self.set_height(height);
    }

    fn set_height(&self, height: u64) {
        if let Some(o) = self.desktop.borrow_mut().as_mut() {
            o.height = height;
        }
    }
}

#[derive(Serialize)]
struct Args<A> {
    args: A,
}
