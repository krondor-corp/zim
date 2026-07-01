//! Browser-side vault reader.
//!
//! Mirrors the daemon's read path against zim-hub's ciphertext API:
//!
//! 1. JS fetches `/api/v0/v/{id}/manifest` and hands the JSON to
//!    [`WasmVault::open`]. We find the share whose pubkey matches
//!    the loaded session key and recover the root [`Secret`] via
//!    [`SecretShare::recover`] — same call the daemon makes.
//! 2. JS fetches `/api/v0/v/{id}/blob/{root_hash}` and calls
//!    [`WasmVault::read_root_dir`].
//! 3. Each returned entry carries the child's blake3 hash + its
//!    per-entry secret; JS fetches the child blob and recurses with
//!    [`WasmVault::read_dir`] / [`WasmVault::read_file`].
//!
//! The dir-body wire format is the DAG-CBOR encoding of
//! `zim_core::fs::Dir` — a map of name → externally-tagged `Entry`
//! enum. The mirror types below deserialize that exact shape with
//! plain serde derive (unknown fields like `metadata` /
//! `plaintext_hash` are skipped by serde's default tolerance), so
//! the wire contract lives in one `#[derive(Deserialize)]` and not
//! in hand-rolled decoding.

use std::collections::BTreeMap;

use ipld_core::cid::Cid;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use zim_crypto::{Secret, SecretShare};

use crate::SESSION_KEY;

/// Mirror of `zim_core::fs::Entry` — only the fields the browser
/// needs. Field names and the external enum tagging must stay in
/// lockstep with zim-core's `Entry`; the dir-body blob is the
/// DAG-CBOR of that type.
#[derive(Deserialize)]
enum Entry {
    File {
        link: Cid,
        secret: Secret,
        #[serde(default)]
        mime: Option<String>,
        /// `blake3(plaintext)` for the file body — raw 32 bytes as a CBOR byte
        /// string. Present on entries written by recent daemons; `None` on
        /// legacy or synthetic entries. Used by `readFile` to confirm the
        /// correct key was used (streaming cipher has no auth tag).
        #[serde(default)]
        plaintext_hash: Option<serde_bytes::ByteBuf>,
    },
    Dir {
        link: Cid,
        secret: Secret,
    },
}

/// Mirror of `zim_core::fs::Dir` (a newtype over the children map —
/// newtype structs serialize transparently, so the map IS the wire
/// format).
type Dir = BTreeMap<String, Entry>;

/// What `read_dir` hands back to JS, as JSON.
#[derive(Serialize)]
struct EntryView {
    name: String,
    kind: &'static str,
    /// blake3 hex — feed to `/api/v0/v/{id}/blob/{hash}`.
    hash: String,
    /// 32-byte hex — feed back to `read_dir` / `read_file`.
    secret: String,
    mime: Option<String>,
    /// blake3 hex of the plaintext body (files only). Feed back to `readFile`
    /// so it can verify the decrypted output. `null` for legacy entries and
    /// all directories.
    plaintext_hash: Option<String>,
}

/// Subset of the hub's `/api/v0/v/{id}/manifest` response that
/// `open` consumes.
#[derive(Deserialize)]
struct ManifestView {
    name: String,
    height: u64,
    root_hash: String,
    shares: Vec<ShareView>,
}

#[derive(Deserialize)]
struct ShareView {
    pubkey: String,
    secret_share: String,
}

#[wasm_bindgen]
pub struct WasmVault {
    name: String,
    height: u64,
    root_hash: String,
    root_secret: Secret,
}

#[wasm_bindgen]
impl WasmVault {
    /// Open a vault from the hub's decoded-manifest JSON. Requires a
    /// session key (`loadKeyFromSession` / `unlockKeyBlob` first) —
    /// the manifest share matching the session key's pubkey is
    /// recovered into the vault's root secret.
    #[wasm_bindgen(js_name = open)]
    pub fn open(manifest_json: &str) -> Result<WasmVault, JsError> {
        let manifest: ManifestView = serde_json::from_str(manifest_json)
            .map_err(|e| JsError::new(&format!("invalid manifest JSON: {e}")))?;

        let our_pubkey = crate::public_key_hex()?;
        let share = manifest
            .shares
            .iter()
            .find(|s| s.pubkey == our_pubkey)
            .ok_or_else(|| {
                JsError::new(&format!(
                    "no share for this key ({our_pubkey}) — ask a vault owner to run \
                     `zim vault <name> shares add` for it"
                ))
            })?;
        let share = SecretShare::from_hex(&share.secret_share)
            .map_err(|e| JsError::new(&format!("invalid share: {e}")))?;

        let root_secret = SESSION_KEY.with(|cell| {
            let borrow = cell.borrow();
            let key = borrow
                .as_ref()
                .ok_or_else(|| JsError::new("no session key loaded"))?;
            share
                .recover(key)
                .map_err(|e| JsError::new(&format!("share recover failed: {e}")))
        })?;

        Ok(WasmVault {
            name: manifest.name,
            height: manifest.height,
            root_hash: manifest.root_hash,
            root_secret,
        })
    }

    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn height(&self) -> u64 {
        self.height
    }

    /// blake3 hex of the encrypted root dir body. JS fetches this
    /// via `/blob/{hash}` and hands the bytes to `readRootDir`.
    #[wasm_bindgen(getter, js_name = rootHash)]
    pub fn root_hash(&self) -> String {
        self.root_hash.clone()
    }

    /// Decrypt + decode the root dir body. Returns a JSON array of
    /// `{name, kind, hash, secret, mime}` entries.
    #[wasm_bindgen(js_name = readRootDir)]
    pub fn read_root_dir(&self, ciphertext: &[u8]) -> Result<String, JsError> {
        decode_dir(&self.root_secret, ciphertext)
    }

    /// Decrypt + decode a subdirectory body using the per-entry
    /// secret its parent listed for it.
    #[wasm_bindgen(js_name = readDir)]
    pub fn read_dir(&self, secret_hex: &str, ciphertext: &[u8]) -> Result<String, JsError> {
        decode_dir(&secret_from_hex(secret_hex)?, ciphertext)
    }

    /// Decrypt a file body. Returns the plaintext bytes.
    ///
    /// File content uses the *streaming* cipher format (12-byte
    /// nonce || raw ChaCha20 keystream — see `Secret::encrypt_reader`,
    /// which `ContentStore::put_file` writes with), NOT the one-shot
    /// AEAD envelope dir bodies use. Pass `plaintext_hash_hex` (the
    /// `plaintext_hash` field from the parent dir listing) to verify
    /// the decrypted output — wrong key yields garbage silently without
    /// this check. `null` skips verification (legacy entries only).
    #[wasm_bindgen(js_name = readFile)]
    pub fn read_file(
        &self,
        secret_hex: &str,
        ciphertext: &[u8],
        plaintext_hash_hex: Option<String>,
    ) -> Result<Vec<u8>, JsError> {
        let secret = secret_from_hex(secret_hex)?;
        decode_file_inner(&secret, ciphertext, plaintext_hash_hex.as_deref())
            .map_err(|e| JsError::new(&e))
    }
}

fn secret_from_hex(hex_str: &str) -> Result<Secret, JsError> {
    let bytes =
        hex::decode(hex_str).map_err(|e| JsError::new(&format!("invalid secret hex: {e}")))?;
    Secret::from_slice(&bytes).map_err(|e| JsError::new(&format!("invalid secret: {e}")))
}

fn decode_dir(secret: &Secret, ciphertext: &[u8]) -> Result<String, JsError> {
    decode_dir_inner(secret, ciphertext).map_err(|e| JsError::new(&e))
}

fn decode_file_inner(
    secret: &Secret,
    ciphertext: &[u8],
    plaintext_hash_hex: Option<&str>,
) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut reader = secret
        .decrypt_reader(std::io::Cursor::new(ciphertext.to_vec()))
        .map_err(|e| format!("file decrypt failed: {e}"))?;
    let mut plaintext = Vec::with_capacity(ciphertext.len().saturating_sub(12));
    reader
        .read_to_end(&mut plaintext)
        .map_err(|e| format!("file read failed: {e}"))?;

    if let Some(expected_hex) = plaintext_hash_hex {
        let expected =
            hex::decode(expected_hex).map_err(|e| format!("plaintext_hash hex invalid: {e}"))?;
        let actual = blake3::hash(&plaintext);
        if actual.as_bytes() != expected.as_slice() {
            return Err(format!(
                "plaintext_hash mismatch: got {} expected {expected_hex}",
                hex::encode(actual.as_bytes())
            ));
        }
    }

    Ok(plaintext)
}

/// Plain-Result inner so host-side tests can exercise both paths —
/// `JsError` can't even be constructed off wasm32.
fn decode_dir_inner(secret: &Secret, ciphertext: &[u8]) -> Result<String, String> {
    let plaintext = secret
        .decrypt(ciphertext)
        .map_err(|e| format!("dir decrypt failed: {e}"))?;
    let dir: Dir = serde_ipld_dagcbor::from_slice(&plaintext)
        .map_err(|e| format!("dir decode failed: {e}"))?;

    let views: Vec<EntryView> = dir
        .into_iter()
        .map(|(name, entry)| match entry {
            Entry::File {
                link,
                secret,
                mime,
                plaintext_hash,
            } => EntryView {
                name,
                kind: "file",
                hash: hex::encode(link.hash().digest()),
                secret: hex::encode(secret.bytes()),
                mime,
                plaintext_hash: plaintext_hash
                    .filter(|b| b.len() == 32)
                    .map(|b| hex::encode(&*b)),
            },
            Entry::Dir { link, secret } => EntryView {
                name,
                kind: "dir",
                hash: hex::encode(link.hash().digest()),
                secret: hex::encode(secret.bytes()),
                mime: None,
                plaintext_hash: None,
            },
        })
        .collect();
    serde_json::to_string(&views).map_err(|e| format!("json encode: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zim_core::fs::{Dir as CoreDir, Entry as CoreEntry};
    use zim_core::linked_data::{BlockEncoded, Hash, Link, LD_RAW_CODEC};

    /// Alice's daemon writes a dir; her browser reads it. Encode with
    /// the REAL zim-core types, decode with the wasm mirror — if the
    /// wire format drifts on either side, this fails loudly.
    #[test]
    fn browser_decodes_what_the_daemon_encoded() {
        let dir_body_secret = Secret::generate();
        let notes_secret = Secret::generate();
        let photos_secret = Secret::generate();

        // — Alice's daemon side: real zim-core construction —
        let notes_hash = Hash::new(b"ciphertext of notes.txt");
        let photos_hash = Hash::new(b"ciphertext of photos dir body");
        let mut dir = CoreDir::new();
        dir.insert(
            "notes.txt".to_string(),
            CoreEntry::file_from_path(
                Link::new(LD_RAW_CODEC, notes_hash),
                notes_secret.clone(),
                std::path::Path::new("notes.txt"),
            ),
        );
        dir.insert(
            "photos".to_string(),
            CoreEntry::dir(Link::new(LD_RAW_CODEC, photos_hash), photos_secret.clone()),
        );
        // Same encode+encrypt the daemon's ContentStore::put_metadata does.
        let ciphertext = dir_body_secret.encrypt(&dir.encode().unwrap()).unwrap();

        // — Alice's browser side: wasm mirror decode —
        let json = decode_dir_inner(&dir_body_secret, &ciphertext)
            .expect("decode failed on real zim-core output");
        let entries: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(entries.len(), 2);

        // BTreeMap ordering: notes.txt < photos.
        let notes = &entries[0];
        assert_eq!(notes["name"], "notes.txt");
        assert_eq!(notes["kind"], "file");
        assert_eq!(notes["hash"], hex::encode(notes_hash.as_bytes()));
        assert_eq!(notes["secret"], hex::encode(notes_secret.bytes()));
        assert_eq!(notes["mime"], "text/plain");

        let photos = &entries[1];
        assert_eq!(photos["name"], "photos");
        assert_eq!(photos["kind"], "dir");
        assert_eq!(photos["hash"], hex::encode(photos_hash.as_bytes()));
        assert_eq!(photos["secret"], hex::encode(photos_secret.bytes()));
        assert!(photos["mime"].is_null());
    }

    /// File content uses the streaming cipher, not the AEAD
    /// envelope. Encode with the same `encrypt_reader` that
    /// `ContentStore::put_file` uses; decode with the wasm side.
    #[test]
    fn browser_decrypts_streamed_file_content() {
        use std::io::Read;
        let secret = Secret::generate();
        let plaintext = b"[package]\nname = \"demo\"\n";

        // Daemon side: put_file's encrypt path.
        let mut encrypted_reader = secret
            .encrypt_reader(std::io::Cursor::new(plaintext.to_vec()))
            .unwrap();
        let mut ciphertext = Vec::new();
        encrypted_reader.read_to_end(&mut ciphertext).unwrap();

        // Browser side.
        // No plaintext_hash provided (legacy path).
        let recovered = decode_file_inner(&secret, &ciphertext, None).expect("file decode");
        assert_eq!(recovered, plaintext);

        // With plaintext_hash: verify match.
        let hash_hex = hex::encode(blake3::hash(plaintext).as_bytes());
        let verified =
            decode_file_inner(&secret, &ciphertext, Some(&hash_hex)).expect("hash verify");
        assert_eq!(verified, plaintext);

        // Wrong hash → error.
        let bad_hex = hex::encode([0u8; 32]);
        assert!(decode_file_inner(&secret, &ciphertext, Some(&bad_hex)).is_err());
    }

    /// Wrong secret → decrypt error, not a panic or garbage decode.
    #[test]
    fn wrong_secret_fails_cleanly() {
        let real = Secret::generate();
        let wrong = Secret::generate();
        let mut dir = CoreDir::new();
        dir.insert(
            "x".to_string(),
            CoreEntry::dir(Link::new(LD_RAW_CODEC, Hash::new(b"x")), Secret::generate()),
        );
        let ciphertext = real.encrypt(&dir.encode().unwrap()).unwrap();
        assert!(decode_dir_inner(&wrong, &ciphertext).is_err());
    }
}
