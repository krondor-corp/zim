//! Browser-side WASM client for Zim.
//!
//! zim-hub serves ciphertext only — bucket secrets never leave the viewer's
//! browser. This crate is loaded by Datastar pages that need to decrypt
//! published encrypted content. The viewer's Ed25519 [`SecretKey`] (when
//! present) is held in WASM linear memory; JS never sees it.
//!
//! See `crates/zim-wasm/README.md` for the build command, envelope schema,
//! and the script-tag wiring pattern used by zim-hub templates.

use std::cell::RefCell;

use serde::Deserialize;
use wasm_bindgen::prelude::*;
use zim_crypto::{Secret, SecretKey, SecretShare};

thread_local! {
    static SESSION_KEY: RefCell<Option<SecretKey>> = const { RefCell::new(None) };
}

#[wasm_bindgen]
pub fn init() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen(js_name = loadKeyFromSession)]
pub fn load_key_from_session(key_bytes: &[u8]) -> Result<(), JsError> {
    let arr: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| JsError::new("key_bytes must be exactly 32 bytes"))?;
    let sk = SecretKey::from(arr);
    SESSION_KEY.with(|s| *s.borrow_mut() = Some(sk));
    Ok(())
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Envelope {
    /// T-008 public path: per-blob `Secret` is in the envelope as hex.
    /// Anonymous viewers — no session key required.
    Public { secret: String },
    /// T-001 member-viewer path: per-blob `Secret` is sealed inside a
    /// `SecretShare` for the viewer's `PublicKey`. Requires `loadKeyFromSession`.
    Sealed { share: String },
}

#[wasm_bindgen(js_name = decryptBlob)]
pub fn decrypt_blob(envelope_json: &str, ciphertext: &[u8]) -> Result<Vec<u8>, JsError> {
    let envelope: Envelope = serde_json::from_str(envelope_json)
        .map_err(|e| JsError::new(&format!("invalid envelope JSON: {e}")))?;

    let secret: Secret = match envelope {
        Envelope::Public { secret } => {
            let bytes = hex::decode(&secret)
                .map_err(|e| JsError::new(&format!("invalid public secret hex: {e}")))?;
            Secret::from_slice(&bytes)
                .map_err(|e| JsError::new(&format!("invalid public secret bytes: {e}")))?
        }
        Envelope::Sealed { share } => {
            let share = SecretShare::from_hex(&share)
                .map_err(|e| JsError::new(&format!("invalid share: {e}")))?;
            SESSION_KEY.with(|cell| {
                let borrow = cell.borrow();
                let key = borrow.as_ref().ok_or_else(|| {
                    JsError::new("no session key loaded; call loadKeyFromSession first")
                })?;
                share
                    .recover(key)
                    .map_err(|e| JsError::new(&format!("share recover failed: {e}")))
            })?
        }
    };

    secret
        .decrypt(ciphertext)
        .map_err(|e| JsError::new(&format!("decrypt failed: {e}")))
}

#[wasm_bindgen(js_name = clearKey)]
pub fn clear_key() {
    SESSION_KEY.with(|s| *s.borrow_mut() = None);
}
