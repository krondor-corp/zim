//! [`WasmVersion`] — a read-only view of a vault as it was at an earlier
//! height.
//!
//! Every manifest pins its whole tree, so the historical head at a height
//! fully reconstructs that version. Decryption uses this browser's share
//! in *that* manifest: versions from before the key was granted don't
//! carry one and fail to open (see `WasmFs::history`'s `readable`).
//!
//! Exposes reads only — there is no `save`, so a historical handle can
//! never chain a new head off a stale version. Restoring is a `cat` here
//! plus an `add_file` + `save` on the live [`WasmFs`](crate::WasmFs).
//!
//! [`WasmVersion::history`] lists the chain to pick a version from.

use reqwest::Client;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use zim_core::blobs::BlobStore;
use zim_core::fs::Manifest;
use zim_core::vault::{Vault, VaultLog};

use crate::api::{self, GetLog};
use crate::fs::{parse_base, parse_vault_id, session_key, HubBlobStore, HubVaultLog};
use crate::read;

#[wasm_bindgen]
pub struct WasmVersion {
    vault: Vault<HubBlobStore, HubVaultLog>,
}

#[wasm_bindgen]
impl WasmVersion {
    /// One page of the version chain, newest first, starting at height
    /// `from` (default: the hub's current head) and walking back at most
    /// `limit` versions. Returns JSON
    /// `[{height, manifest_hash, author, access}]`.
    ///
    /// Each version's manifest is fetched for its plaintext header (no
    /// decryption): `author` is the signing key, and `access` is
    /// - `"readable"` — this browser's key holds a share in that version;
    /// - `"sealed"` — it doesn't (a key granted later can't open versions
    ///   from before its grant). Permanent;
    /// - `"unavailable"` — the manifest couldn't be fetched (network, hub
    ///   hiccup). Says nothing about access; opening it again may work.
    pub async fn history(
        vault_id_hex: String,
        hub_base: String,
        from: Option<u64>,
        limit: u32,
    ) -> Result<String, JsError> {
        let me = session_key()?.public();
        let base = parse_base(&hub_base)?;
        let id = parse_vault_id(&vault_id_hex)?;
        let from = match from {
            Some(h) => h,
            None => HubVaultLog::new(base.clone())
                .height(id)
                .await
                .map_err(|e| JsError::new(&format!("history: {e}")))?,
        };
        let resp = api::call(
            &base,
            &Client::new(),
            GetLog {
                id,
                from,
                limit: u64::from(limit),
            },
        )
        .await
        .map_err(|e| JsError::new(&format!("history: {e}")))?;

        #[derive(Serialize)]
        struct Version {
            height: u64,
            manifest_hash: String,
            author: Option<String>,
            access: &'static str,
        }

        let blobs = HubBlobStore::new(base);
        let manifests: Vec<Result<Manifest, _>> =
            futures::future::join_all(resp.entries.iter().map(|e| blobs.get_cbor(&e.link))).await;
        // A manifest the hub can't serve still lists — without its header,
        // and marked unavailable rather than sealed: a fetch failure is no
        // evidence about access.
        let versions: Vec<Version> = resp
            .entries
            .into_iter()
            .zip(manifests)
            .map(|(e, m)| {
                let (author, access) = match m {
                    Ok(m) if m.get_share(&me).is_some() => (Some(m.author().to_hex()), "readable"),
                    Ok(m) => (Some(m.author().to_hex()), "sealed"),
                    Err(_) => (None, "unavailable"),
                };
                Version {
                    height: e.height,
                    manifest_hash: e.link.hash().to_hex(),
                    author,
                    access,
                }
            })
            .collect();
        serde_json::to_string(&versions).map_err(|e| JsError::new(&format!("json: {e}")))
    }

    /// Open vault `vault_id_hex` at `height` (the canonical head there).
    pub async fn open(
        vault_id_hex: String,
        height: u64,
        hub_base: String,
    ) -> Result<WasmVersion, JsError> {
        let pk = session_key()?;
        let base = parse_base(&hub_base)?;
        let vault = Vault::open_at(
            parse_vault_id(&vault_id_hex)?,
            height,
            HubBlobStore::new(base.clone()),
            HubVaultLog::new(base),
            &pk,
        )
        .await
        .map_err(|e| JsError::new(&format!("open version {height}: {e}")))?;
        Ok(WasmVersion { vault })
    }

    /// This version's header as JSON: `{height, manifest_hash, author}`.
    pub fn info(&self) -> Result<String, JsError> {
        #[derive(Serialize)]
        struct Info {
            height: u64,
            manifest_hash: String,
            author: String,
        }
        serde_json::to_string(&Info {
            height: self.vault.height(),
            manifest_hash: self.vault.manifest_link().hash().to_hex(),
            author: self.vault.manifest().author().to_hex(),
        })
        .map_err(|e| JsError::new(&format!("json: {e}")))
    }

    /// List a directory as it was. JSON: `[{name, kind, hash, mime}]`.
    pub async fn ls(&self, path: String) -> Result<String, JsError> {
        read::ls(self.vault.fs(), &path).await
    }

    /// A file's plaintext as it was at this version.
    pub async fn cat(&self, path: String) -> Result<Vec<u8>, JsValue> {
        read::cat(self.vault.fs(), &path).await
    }
}
