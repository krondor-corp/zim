//! Browser-side vault — a real networked `Vault` over the hub's HTTP API.
//!
//! [`WasmFs`] wraps `Vault<HubBlobStore, HubVaultLog>`. The two backends are
//! the browser equivalents of the daemon's local stores, and both hold
//! nothing but a base [`Url`] + a `reqwest::Client`: every request is a typed
//! route dispatched through [`crate::api::call`], which owns auth + transport.
//!
//! This module is wasm32-only (reqwest's wasm client needs a browser
//! `fetch`); `lib.rs` gates it behind `#[cfg(target_arch = "wasm32")]`.

use std::io::Read;
use std::str::FromStr;

use async_trait::async_trait;
use bytes::Bytes;
use reqwest::{Client, StatusCode};
use serde::Serialize;
use url::Url;
use wasm_bindgen::prelude::*;

use zim_core::blobs::{BlobError, BlobStore};
use zim_core::linked_data::{Hash, Link};
use zim_core::vault::log::{Head, VaultLog, VaultLogError};
use zim_core::vault::{Vault, VaultId};

use crate::api::{self, GetBlob, GetHead, GetLog, PostHead, PutBlob};
use crate::{read, SESSION_KEY};

fn blob_err(e: String) -> BlobError {
    BlobError::Store(anyhow::anyhow!(e))
}

// ---------------------------------------------------------------------------
// HubBlobStore — stateless content store over /api/v0/blob.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct HubBlobStore {
    client: Client,
    base: Url,
}

impl HubBlobStore {
    pub(crate) fn new(base: Url) -> Self {
        Self {
            client: Client::new(),
            base,
        }
    }
}

#[async_trait]
impl BlobStore for HubBlobStore {
    async fn get(&self, hash: &Hash) -> Result<Bytes, BlobError> {
        match api::try_call(&self.base, &self.client, GetBlob(*hash)).await {
            Ok(bytes) => Ok(bytes),
            Err(e) if e.status == Some(StatusCode::NOT_FOUND) => Err(BlobError::NotFound(*hash)),
            Err(e) => Err(blob_err(e.message)),
        }
    }

    async fn put(&self, data: Vec<u8>) -> Result<Hash, BlobError> {
        let resp = api::call(&self.base, &self.client, PutBlob(data))
            .await
            .map_err(blob_err)?;
        Hash::from_str(&resp.hash).map_err(|e| blob_err(e.to_string()))
    }

    async fn put_reader(
        &self,
        mut reader: Box<dyn Read + Send + 'static>,
    ) -> Result<Hash, BlobError> {
        let mut buf = Vec::new();
        reader
            .read_to_end(&mut buf)
            .map_err(|e| BlobError::Store(e.into()))?;
        self.put(buf).await
    }

    async fn stat(&self, hash: &Hash) -> Result<bool, BlobError> {
        match self.get(hash).await {
            Ok(_) => Ok(true),
            Err(BlobError::NotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

// ---------------------------------------------------------------------------
// HubVaultLog — per-vault head log over /api/v0/vaults/{id}/...
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct HubVaultLog {
    client: Client,
    base: Url,
}

impl HubVaultLog {
    pub(crate) fn new(base: Url) -> Self {
        Self {
            client: Client::new(),
            base,
        }
    }
}

#[async_trait]
impl VaultLog for HubVaultLog {
    type Error = String;

    async fn head(&self, id: VaultId, height: Option<u64>) -> Result<Head, VaultLogError<String>> {
        // The hub's /head endpoint hands back the canonical head + height
        // in one round-trip; use it for "current" reads. Historical reads
        // at an explicit height fall through to the height-indexed walk.
        if height.is_none() {
            let resp = api::call(&self.base, &self.client, GetHead(id))
                .await
                .map_err(VaultLogError::Provider)?;
            return Ok(Head::new(resp.link, resp.height));
        }
        let height = height.expect("checked Some above");
        self.heads(id, height)
            .await?
            .into_iter()
            .max()
            .map(|link| Head::new(link, height))
            .ok_or(VaultLogError::HeadNotFound(height))
    }

    async fn exists(&self, id: VaultId) -> Result<bool, VaultLogError<String>> {
        Ok(api::call(&self.base, &self.client, GetHead(id))
            .await
            .is_ok())
    }

    async fn height(&self, id: VaultId) -> Result<u64, VaultLogError<String>> {
        let resp = api::call(&self.base, &self.client, GetHead(id))
            .await
            .map_err(VaultLogError::Provider)?;
        Ok(resp.height)
    }

    async fn heads(&self, id: VaultId, height: u64) -> Result<Vec<Link>, VaultLogError<String>> {
        let resp = api::call(
            &self.base,
            &self.client,
            GetLog {
                id,
                from: height,
                limit: 1,
            },
        )
        .await
        .map_err(VaultLogError::Provider)?;
        Ok(resp
            .entries
            .into_iter()
            .filter(|e| e.height == height)
            .map(|e| e.link)
            .collect())
    }

    async fn append(
        &self,
        id: VaultId,
        _name: String,
        current: Link,
        _prev: Option<Link>,
        _height: u64,
    ) -> Result<(), VaultLogError<String>> {
        api::call(
            &self.base,
            &self.client,
            PostHead {
                id,
                manifest_hash: current.hash().to_hex(),
            },
        )
        .await
        .map_err(VaultLogError::Provider)
    }

    async fn has(&self, id: VaultId, link: Link) -> Result<Vec<u64>, VaultLogError<String>> {
        let top = self.height(id).await?;
        let resp = api::call(
            &self.base,
            &self.client,
            GetLog {
                id,
                from: top,
                limit: 100,
            },
        )
        .await
        .map_err(VaultLogError::Provider)?;
        Ok(resp
            .entries
            .into_iter()
            .filter(|e| e.link == link)
            .map(|e| e.height)
            .collect())
    }

    async fn list_vaults(&self) -> Result<Vec<VaultId>, VaultLogError<String>> {
        // The browser never enumerates the hub's whole log; it opens
        // vaults it already knows by id.
        Ok(Vec::new())
    }
}

// ---------------------------------------------------------------------------
// WasmFs — the JS-facing handle.
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub struct WasmFs {
    vault: Vault<HubBlobStore, HubVaultLog>,
}

pub(crate) fn session_key() -> Result<zim_crypto::PrivateKey, JsError> {
    SESSION_KEY.with(|cell| {
        cell.borrow()
            .as_ref()
            .cloned()
            .ok_or_else(|| JsError::new("no session key loaded; unlock first"))
    })
}

pub(crate) fn parse_vault_id(hex: &str) -> Result<VaultId, JsError> {
    Ok(VaultId::from_hash(
        Hash::from_str(hex).map_err(|e| JsError::new(&format!("vault id: {e}")))?,
    ))
}

pub(crate) fn parse_base(hub_base: &str) -> Result<Url, JsError> {
    hub_base
        .parse()
        .map_err(|e| JsError::new(&format!("invalid hub url: {e}")))
}

#[wasm_bindgen]
impl WasmFs {
    /// Initialise a brand-new vault on the hub and share it with every device
    /// enrolled to `user_id`. We resolve the user's did:web document to its
    /// full key set and seal the vault secret to every device right in the
    /// genesis manifest — one head, no chain advance — so the owner's other
    /// devices (daemons + other browsers) have access from the first commit.
    /// The owner share is stamped with the hub as its `via` so daemons reach
    /// this browser through the hub. Resolution failure is surfaced, not
    /// swallowed.
    pub async fn init(name: String, hub_base: String, user_id: String) -> Result<WasmFs, JsError> {
        let pk = session_key()?;
        let base = parse_base(&hub_base)?;

        // Resolve the user's did:web document to its full enrolled key set
        // *first*, then seal the vault secret to every device right in the
        // genesis manifest. One head, no chain advance — so the hub has
        // nothing to conflict on (it accepts a genesis unconditionally).
        // A resolution failure is surfaced, not swallowed.
        let keys = crate::api::resolve_user_keys(&base, &user_id)
            .await
            .map_err(|e| JsError::new(&format!("share with your devices failed: {e}")))?;

        // The owner is *this browser's* web key — reachable only through the
        // hub, never dialed directly over iroh. Seal the genesis owner share
        // with the hub as its `via`, so a daemon that advances the vault
        // announces to the hub (which mirrors the new head for the browser to
        // read) instead of trying to dial the browser and failing.
        let hub_key = crate::api::resolve_hub_key(&base)
            .await
            .map_err(|e| JsError::new(&format!("resolve hub key: {e}")))?;
        let owner_via = Some(zim_did::Did::from_key(&hub_key));
        // The did.json doesn't say which keys are daemons vs browsers, so
        // every device is sealed as directly dialed.
        let shares: Vec<_> = keys.into_iter().map(|k| (k, None)).collect();

        let vault = Vault::init_with_shares(
            name,
            &pk,
            owner_via,
            &shares,
            HubBlobStore::new(base.clone()),
            HubVaultLog::new(base),
        )
        .await
        .map_err(|e| JsError::new(&format!("init: {e}")))?;

        web_sys::console::log_1(
            &format!(
                "vault init: genesis sealed to {} device key(s) for user {user_id}",
                vault.manifest().shares().iter().count()
            )
            .into(),
        );
        Ok(WasmFs { vault })
    }

    /// Open an existing vault by id. Fetches the head from the hub, pulls
    /// the manifest blob, recovers the local share, and materialises the
    /// tree. Blobs are fetched lazily thereafter.
    pub async fn open(vault_id_hex: String, hub_base: String) -> Result<WasmFs, JsError> {
        let pk = session_key()?;
        let base = parse_base(&hub_base)?;
        let vault = Vault::open(
            parse_vault_id(&vault_id_hex)?,
            HubBlobStore::new(base.clone()),
            HubVaultLog::new(base),
            &pk,
        )
        .await
        .map_err(|e| JsError::new(&format!("open: {e}")))?;
        Ok(WasmFs { vault })
    }

    #[wasm_bindgen(getter)]
    pub fn vault_id(&self) -> String {
        self.vault.id().to_string()
    }

    /// Vault metadata + shareholders for the Details panel, as JSON:
    /// `{vault_id, name, height, manifest_hash, author, shares: [{pubkey, did, via}]}`.
    /// Reads the already-open manifest — no network.
    pub fn manifest_info(&self) -> Result<String, JsError> {
        let manifest = self.vault.manifest();

        #[derive(Serialize)]
        struct ShareView {
            pubkey: String,
            did: String,
            via: Option<String>,
        }
        #[derive(Serialize)]
        struct Info {
            vault_id: String,
            name: String,
            height: u64,
            manifest_hash: String,
            author: String,
            shares: Vec<ShareView>,
        }

        let shares: Vec<ShareView> = manifest
            .shares()
            .iter()
            .map(|(pk, share)| ShareView {
                pubkey: pk.to_hex(),
                did: share.identity().to_string(),
                via: share.via().map(|v| v.to_string()),
            })
            .collect();

        let info = Info {
            vault_id: self.vault.id().to_string(),
            name: manifest.name().to_string(),
            height: manifest.height(),
            manifest_hash: self.vault.manifest_link().hash().to_string(),
            author: manifest.author().to_hex(),
            shares,
        };
        serde_json::to_string(&info).map_err(|e| JsError::new(&format!("manifest_info: {e}")))
    }

    /// List a directory. Returns JSON: `[{name, kind, hash, mime}]`.
    pub async fn ls(&self, path: String) -> Result<String, JsError> {
        read::ls(self.vault.fs(), &path).await
    }

    /// Read and decrypt a file's plaintext bytes. When the tree points at
    /// a blob the hub hasn't mirrored yet (another device just wrote it),
    /// the error's `name` is [`BLOB_PENDING`](crate::BLOB_PENDING) so the UI can show "syncing"
    /// and retry, rather than treat the file as empty or broken.
    pub async fn cat(&self, path: String) -> Result<Vec<u8>, JsValue> {
        read::cat(self.vault.fs(), &path).await
    }

    pub async fn add_file(&self, path: String, plaintext: Vec<u8>) -> Result<(), JsError> {
        let abs = zim_core::fs::AbsPath::new(&path)
            .ok_or_else(|| JsError::new(&format!("invalid path: {path}")))?;
        self.vault
            .fs()
            .add(&abs, std::io::Cursor::new(plaintext))
            .await
            .map_err(|e| JsError::new(&format!("add: {e}")))
    }

    pub async fn mkdir(&self, path: String) -> Result<(), JsError> {
        let abs = zim_core::fs::AbsPath::new(&path)
            .ok_or_else(|| JsError::new(&format!("invalid path: {path}")))?;
        self.vault
            .fs()
            .mkdir(&abs, true)
            .await
            .map_err(|e| JsError::new(&format!("mkdir: {e}")))
    }

    pub async fn rm(&self, path: String) -> Result<(), JsError> {
        let abs = zim_core::fs::AbsPath::new(&path)
            .ok_or_else(|| JsError::new(&format!("invalid path: {path}")))?;
        self.vault
            .fs()
            .rm(&abs)
            .await
            .map_err(|e| JsError::new(&format!("rm: {e}")))
    }

    pub async fn mv(&self, from: String, to: String) -> Result<(), JsError> {
        let from = zim_core::fs::AbsPath::new(&from)
            .ok_or_else(|| JsError::new(&format!("invalid path: {from}")))?;
        let to = zim_core::fs::AbsPath::new(&to)
            .ok_or_else(|| JsError::new(&format!("invalid path: {to}")))?;
        self.vault
            .fs()
            .mv(&from, &to)
            .await
            .map_err(|e| JsError::new(&format!("mv: {e}")))
    }

    /// Fast-forward to the hub's current head if it moved (another device
    /// wrote). Returns `true` when the tree changed. Staged, unsaved
    /// mutations are discarded on a real refresh — save first, or replay.
    pub async fn refresh(&mut self) -> Result<bool, JsError> {
        self.vault
            .refresh()
            .await
            .map_err(|e| JsError::new(&format!("refresh: {e}")))
    }

    /// Persist staged mutations. `Vault::save` uploads the new manifest
    /// blob and any new file blobs via `HubBlobStore`, then `POST`s the new
    /// head via `HubVaultLog` — all inline. Returns JSON
    /// `{vault_id, height, manifest_hash}`.
    pub async fn save(&mut self) -> Result<String, JsError> {
        let link = self
            .vault
            .save()
            .await
            .map_err(|e| JsError::new(&format!("save: {e}")))?;

        #[derive(Serialize)]
        struct Saved {
            vault_id: String,
            height: u64,
            manifest_hash: String,
        }
        serde_json::to_string(&Saved {
            vault_id: self.vault.id().to_string(),
            height: self.vault.height(),
            manifest_hash: link.hash().to_hex(),
        })
        .map_err(|e| JsError::new(&format!("json: {e}")))
    }
}
