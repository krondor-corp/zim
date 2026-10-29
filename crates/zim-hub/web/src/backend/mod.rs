//! The vault page's storage seam: every read and write the editor makes
//! goes through [`VaultBackend`], never a `WasmFs` directly. Two
//! backends sit behind it, chosen at runtime: the hub — the SDK's
//! `WasmFs`, with all blob crypto inside it — and, inside the Tauri
//! desktop shell, the local daemon via [`desktop`]. Both speak the same
//! domain types (`VaultId`, `DirEntry`, `VaultInfo`).
//!
//! The handle lives in a `RefCell` shared by user actions, autosave, and
//! the background follow loop. Async methods wait out an in-flight borrow
//! ([`lock`], [`lock_mut`]) instead of panicking on it, and hold their
//! borrow across the SDK's awaits on purpose: that is what serializes
//! writes against each other and against reads.

use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

use zim_core::vault::VaultId;
use zim_wasm::{DirEntry, VaultInfo, WasmFs, BLOB_PENDING};

use crate::util::{jserr, origin, sleep};

pub mod desktop;
pub use desktop::is_desktop;
pub use zim_api::desktop::VaultSummary;

/// Why a read came back without bytes.
pub enum ReadError {
    /// The tree names a blob the hub hasn't mirrored yet — another
    /// device just wrote it. Retry shortly; don't show an empty file.
    Pending,
    Failed(String),
}

/// What the handle is doing right now, without waiting on it.
#[derive(Clone, Copy, PartialEq)]
pub enum BackendState {
    /// No vault open yet.
    Closed,
    /// A vault is open and nothing holds it.
    Idle,
    /// A task holds the open vault across an await.
    Busy,
}

/// The open vault, shared by every task on the page.
#[derive(Clone)]
pub struct VaultBackend {
    fs: Rc<RefCell<Option<WasmFs>>>,
    /// The open vault when running in the desktop shell. Only read
    /// briefly (never held across an await): the daemon serializes
    /// writes itself.
    desktop: Rc<RefCell<Option<desktop::Opened>>>,
    is_desktop: bool,
}

impl Default for VaultBackend {
    fn default() -> Self {
        Self {
            fs: Rc::default(),
            desktop: Rc::default(),
            is_desktop: is_desktop(),
        }
    }
}

/// The vaults to offer: the hub's mirror, or the local daemon's.
pub async fn list_vaults() -> Result<Vec<VaultSummary>, String> {
    if is_desktop() {
        return desktop::list_vaults().await;
    }
    // The hub's listing carries ids as strings; one that doesn't parse
    // can't be opened, so it's left out.
    Ok(crate::api::fetch_vaults()
        .await?
        .into_iter()
        .filter_map(|v| {
            Some(VaultSummary {
                id: v.vault_id.parse().ok()?,
                name: v.name,
            })
        })
        .collect())
}

/// Create a vault named `name`, returning its id. On the hub the
/// browser key is the owner and shares with all of `user_id`'s devices;
/// on the desktop the daemon creates it (and shares per its hub link).
pub async fn create_vault(name: String, user_id: String) -> Result<VaultId, String> {
    if is_desktop() {
        return desktop::create_vault(name).await;
    }
    let fs = WasmFs::init(name, origin(), user_id)
        .await
        .map_err(|e| jserr(e.into()))?;
    Ok(fs.id())
}

impl PartialEq for VaultBackend {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.fs, &other.fs)
    }
}

/// How long [`lock`]/[`lock_mut`] wait before re-trying a busy handle.
const LOCK_RETRY_MS: i32 = 25;

impl VaultBackend {
    pub fn state(&self) -> BackendState {
        if self.is_desktop {
            return match self.desktop.try_borrow() {
                Ok(g) if g.is_some() => BackendState::Idle,
                _ => BackendState::Closed,
            };
        }
        match self.fs.try_borrow() {
            Ok(g) if g.is_some() => BackendState::Idle,
            Ok(_) => BackendState::Closed,
            Err(_) => BackendState::Busy,
        }
    }

    /// Open `vault_id`, replacing whichever vault was open.
    pub async fn open(&self, id: VaultId) -> Result<(), String> {
        if self.is_desktop {
            return self.desktop_open(id).await;
        }
        let opened = WasmFs::open(id, &origin())
            .await
            .map_err(|e| jserr(e.into()))?;
        *self.lock_mut().await = Some(opened);
        Ok(())
    }

    /// The open vault's manifest metadata (synchronous — no network).
    /// `None` while closed or busy.
    pub fn meta(&self) -> Option<VaultInfo> {
        // The daemon API has no manifest endpoint yet (author, shares).
        if self.is_desktop {
            return None;
        }
        let guard = self.fs.try_borrow().ok()?;
        Some(guard.as_ref()?.info())
    }

    #[allow(clippy::await_holding_refcell_ref)] // see module docs
    pub async fn ls(&self, path: String) -> Result<Vec<DirEntry>, String> {
        if self.is_desktop {
            return self.desktop_ls(&path).await;
        }
        let guard = self.lock().await;
        let f = guard.as_ref().ok_or("vault not open")?;
        f.ls(&path).await.map_err(|e| jserr(e.into()))
    }

    #[allow(clippy::await_holding_refcell_ref)] // see module docs
    pub async fn cat(&self, path: String) -> Result<Vec<u8>, ReadError> {
        if self.is_desktop {
            return self.desktop_cat(&path).await;
        }
        let guard = self.lock().await;
        let f = guard
            .as_ref()
            .ok_or_else(|| ReadError::Failed("vault not open".into()))?;
        f.cat(path).await.map_err(|e| {
            if js_sys::Error::from(e.clone()).name() == BLOB_PENDING {
                ReadError::Pending
            } else {
                ReadError::Failed(jserr(e))
            }
        })
    }

    /// Sync to the hub's head if another device advanced it; `true` when
    /// the tree changed. Best-effort: a failed refresh (offline,
    /// transient) keeps the current view rather than breaking navigation.
    #[allow(clippy::await_holding_refcell_ref)] // see module docs
    pub async fn refresh(&self) -> bool {
        if self.is_desktop {
            return self.desktop_refresh().await;
        }
        let mut guard = self.lock_mut().await;
        match guard.as_mut() {
            Some(f) => f.refresh().await.unwrap_or(false),
            None => false,
        }
    }

    /// Write `bytes` at `path` (create or replace) as a new version.
    pub async fn write(&self, path: String, bytes: Vec<u8>) -> Result<(), String> {
        if self.is_desktop {
            return self.desktop_write(&path, bytes).await;
        }
        self.save_with_retry(async |f: &mut WasmFs| {
            f.add_file(path.clone(), bytes.clone())
                .await
                .map_err(|e| jserr(e.into()))
        })
        .await
    }

    pub async fn mkdir(&self, path: String) -> Result<(), String> {
        if self.is_desktop {
            return self.desktop_mkdir(&path).await;
        }
        self.save_with_retry(async |f: &mut WasmFs| {
            f.mkdir(path.clone()).await.map_err(|e| jserr(e.into()))
        })
        .await
    }

    pub async fn rm(&self, path: String) -> Result<(), String> {
        if self.is_desktop {
            return self.desktop_rm(&path).await;
        }
        self.save_with_retry(async |f: &mut WasmFs| {
            f.rm(path.clone()).await.map_err(|e| jserr(e.into()))
        })
        .await
    }

    pub async fn mv(&self, from: String, to: String) -> Result<(), String> {
        if self.is_desktop {
            return self.desktop_mv(&from, &to).await;
        }
        self.save_with_retry(async |f: &mut WasmFs| {
            f.mv(from.clone(), to.clone())
                .await
                .map_err(|e| jserr(e.into()))
        })
        .await
    }

    /// Run `op` + save; on a head conflict (another device wrote since we
    /// loaded — the hub answers 409), fast-forward and replay once.
    #[allow(clippy::await_holding_refcell_ref)] // see module docs
    async fn save_with_retry<F>(&self, op: F) -> Result<(), String>
    where
        F: for<'a> AsyncFn(&'a mut WasmFs) -> Result<(), String>,
    {
        let mut guard = self.lock_mut().await;
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

    /// Borrow the handle, waiting while another task holds it mutably
    /// across an await (a save, or the follow loop's refresh).
    async fn lock(&self) -> Ref<'_, Option<WasmFs>> {
        loop {
            if let Ok(guard) = self.fs.try_borrow() {
                return guard;
            }
            sleep(LOCK_RETRY_MS).await;
        }
    }

    /// Mutably borrow the handle, waiting out any in-flight borrow.
    async fn lock_mut(&self) -> RefMut<'_, Option<WasmFs>> {
        loop {
            if let Ok(guard) = self.fs.try_borrow_mut() {
                return guard;
            }
            sleep(LOCK_RETRY_MS).await;
        }
    }
}
