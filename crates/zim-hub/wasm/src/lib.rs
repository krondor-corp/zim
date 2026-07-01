//! Browser-side WASM client for Zim.
//!
//! zim-hub serves ciphertext only — bucket secrets never leave the viewer's
//! browser. This crate is loaded by Datastar pages that need to decrypt
//! published encrypted content. The viewer's Ed25519 [`PrivateKey`] (when
//! present) is held in WASM linear memory; JS never sees it.
//!
//! Identity-vault mode (T-001): `generate_key` / `encrypt_key_blob` /
//! `unlock_key_blob` provide the client-side enrol / login / re-key flow.
//! The hub stores only the encrypted blob + Argon2 salt; the password and
//! plaintext secret never leave the browser.
//!
//! See `crates/zim-wasm/README.md` for the build command, envelope schema,
//! identity-vault flow, and the script-tag wiring pattern used by zim-hub
//! templates.

use std::cell::RefCell;

use argon2::{Algorithm, Argon2, Params, Version};
use serde::Deserialize;
use wasm_bindgen::prelude::*;
use zim_crypto::{PrivateKey, Secret, SecretShare};

thread_local! {
    pub(crate) static SESSION_KEY: RefCell<Option<PrivateKey>> = const { RefCell::new(None) };
}

/// Argon2id parameters per T-001 Decision 4 (OWASP 2024 defaults).
const ARGON2_M_COST_KIB: u32 = 19_456;
const ARGON2_T_COST: u32 = 2;
const ARGON2_P_COST: u32 = 1;
const KEK_LEN: usize = 32;
const SALT_LEN: usize = 16;

fn argon2() -> Result<Argon2<'static>, JsError> {
    let params = Params::new(
        ARGON2_M_COST_KIB,
        ARGON2_T_COST,
        ARGON2_P_COST,
        Some(KEK_LEN),
    )
    .map_err(|e| JsError::new(&format!("argon2 params: {e}")))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

fn derive_kek(password: &str, salt: &[u8]) -> Result<Secret, JsError> {
    let mut kek = [0u8; KEK_LEN];
    argon2()?
        .hash_password_into(password.as_bytes(), salt, &mut kek)
        .map_err(|e| JsError::new(&format!("argon2 derive: {e}")))?;
    Secret::from_slice(&kek).map_err(|e| JsError::new(&format!("kek as secret: {e}")))
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
    let sk = PrivateKey::from(arr);
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

/// Hex of the session key's public key — lets JS match the right
/// share in a manifest without exporting the secret.
#[wasm_bindgen(js_name = publicKeyHex)]
pub fn public_key_hex() -> Result<String, JsError> {
    SESSION_KEY.with(|cell| {
        let borrow = cell.borrow();
        let key = borrow
            .as_ref()
            .ok_or_else(|| JsError::new("no session key loaded"))?;
        Ok(key.public().to_hex())
    })
}

// ---------------------------------------------------------------------------
// WasmVault — browser-side vault reader (T-018)
// ---------------------------------------------------------------------------

// The hub HTTP client + networked vault speak reqwest's `fetch` backend,
// so they only exist on the wasm32 target. Nothing native links them (the
// hub serves the pre-built wasm bundle), so there's no host-side stub.
#[cfg(target_arch = "wasm32")]
mod api;
#[cfg(target_arch = "wasm32")]
pub use api::{HubClient, LocalKey};

#[cfg(target_arch = "wasm32")]
mod fs;
#[cfg(target_arch = "wasm32")]
pub use fs::WasmFs;

// ---------------------------------------------------------------------------
// Device model / JWT auth (T-017 / T-017b)
// ---------------------------------------------------------------------------

fn base64url_encode(data: &[u8]) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD.encode(data)
}

/// Sign an EdDSA JWT (compact JWS) using the session key.
///
/// `claims_json` must be a JSON object containing at least `device_id`
/// (used as the `kid` in the JWT header). The returned string is the
/// compact serialization `base64url(header).base64url(payload).base64url(sig)`.
#[wasm_bindgen(js_name = signJwt)]
pub fn sign_jwt(claims_json: &str) -> Result<String, JsError> {
    let claims: serde_json::Value = serde_json::from_str(claims_json)
        .map_err(|e| JsError::new(&format!("invalid claims JSON: {e}")))?;
    let device_id = claims
        .get("device_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsError::new("claims must contain a string device_id"))?;

    let header = serde_json::json!({"alg": "EdDSA", "kid": device_id});
    let header_b64 = base64url_encode(
        &serde_json::to_vec(&header).map_err(|e| JsError::new(&format!("header encode: {e}")))?,
    );
    let payload_b64 = base64url_encode(claims_json.as_bytes());

    let signing_input = format!("{header_b64}.{payload_b64}");
    let signature = SESSION_KEY.with(|cell| {
        let borrow = cell.borrow();
        let key = borrow
            .as_ref()
            .ok_or_else(|| JsError::new("no session key loaded; call unlockKeyBlob first"))?;
        Ok::<_, JsError>(key.sign(signing_input.as_bytes()))
    })?;
    let sig_b64 = base64url_encode(&signature.to_bytes());

    Ok(format!("{header_b64}.{payload_b64}.{sig_b64}"))
}

/// Sign a device-approval payload for the push-approval bootstrap flow.
///
/// Produces an ed25519 signature over `pending_id || new_pubkey || expiry`
/// (big-endian u32 for expiry). The hub verifies this signature against the
/// approving device's pubkey before promoting the pending device.
#[wasm_bindgen(js_name = signApproval)]
pub fn sign_approval(
    pending_id: &str,
    new_pubkey: &[u8],
    expiry_unix: u32,
) -> Result<Vec<u8>, JsError> {
    let mut msg = Vec::with_capacity(pending_id.len() + new_pubkey.len() + 4);
    msg.extend_from_slice(pending_id.as_bytes());
    msg.extend_from_slice(new_pubkey);
    msg.extend_from_slice(&expiry_unix.to_be_bytes());

    SESSION_KEY.with(|cell| {
        let borrow = cell.borrow();
        let key = borrow
            .as_ref()
            .ok_or_else(|| JsError::new("no session key loaded; call unlockKeyBlob first"))?;
        Ok(key.sign(&msg).to_bytes().to_vec())
    })
}

/// Sign a device-enrolment challenge — the possession proof the hub's
/// `/api/v0/devices/self` endpoint verifies. Signs
/// `challenge_bytes || session_pubkey_bytes` with the loaded session key;
/// `challenge_hex` is the hub-issued challenge. Returns hex(signature).
#[wasm_bindgen(js_name = signEnrollChallenge)]
pub fn sign_enroll_challenge(challenge_hex: &str) -> Result<String, JsError> {
    let mut msg = hex::decode(challenge_hex)
        .map_err(|e| JsError::new(&format!("invalid challenge hex: {e}")))?;
    SESSION_KEY.with(|cell| {
        let borrow = cell.borrow();
        let key = borrow
            .as_ref()
            .ok_or_else(|| JsError::new("no session key loaded; call generateKey first"))?;
        msg.extend_from_slice(&key.public().to_bytes());
        Ok(hex::encode(key.sign(&msg).to_bytes()))
    })
}

/// Hex of the loaded session key's 32-byte seed, for caching in the tab's
/// `sessionStorage` so navigation doesn't re-prompt for the passphrase.
/// Only the *encrypted* blob is persisted at rest (see [`encrypt_key_blob`]);
/// this value lives solely in tab-scoped memory and is re-loadable via
/// [`load_key_from_session`].
#[wasm_bindgen(js_name = sessionSeedHex)]
pub fn session_seed_hex() -> Result<String, JsError> {
    SESSION_KEY.with(|cell| {
        let borrow = cell.borrow();
        let key = borrow
            .as_ref()
            .ok_or_else(|| JsError::new("no session key loaded"))?;
        Ok(hex::encode(key.to_bytes()))
    })
}

// ---------------------------------------------------------------------------
// Identity vault (T-001 / T-001b)
// ---------------------------------------------------------------------------

/// Generate a fresh viewer keypair, store the secret in the session, return
/// the public key bytes for hub-side enrolment.
#[wasm_bindgen(js_name = generateKey)]
pub fn generate_key() -> Vec<u8> {
    let sk = PrivateKey::generate();
    let pk_bytes = sk.public().to_bytes().to_vec();
    SESSION_KEY.with(|s| *s.borrow_mut() = Some(sk));
    pk_bytes
}

/// Result of [`encrypt_key_blob`]. Holds the values the hub needs to persist
/// for a viewer's identity-vault entry.
#[wasm_bindgen]
pub struct KeyBlob {
    encrypted_blob: Vec<u8>,
    salt: Vec<u8>,
    public_key: Vec<u8>,
}

#[wasm_bindgen]
impl KeyBlob {
    #[wasm_bindgen(getter, js_name = encryptedBlob)]
    pub fn encrypted_blob(&self) -> Vec<u8> {
        self.encrypted_blob.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn salt(&self) -> Vec<u8> {
        self.salt.clone()
    }

    #[wasm_bindgen(getter, js_name = publicKey)]
    pub fn public_key(&self) -> Vec<u8> {
        self.public_key.clone()
    }
}

/// Encrypt the currently-loaded session key with a password-derived KEK and
/// return the artefacts the hub needs to store (`encrypted_blob`, `salt`,
/// `public_key`). The session key remains loaded.
///
/// Errors if no session key is loaded (call [`generate_key`] or
/// [`unlock_key_blob`] first) or if randomness / KDF / AEAD fail.
#[wasm_bindgen(js_name = encryptKeyBlob)]
pub fn encrypt_key_blob(password: &str) -> Result<KeyBlob, JsError> {
    let mut salt = [0u8; SALT_LEN];
    getrandom::fill(&mut salt).map_err(|e| JsError::new(&format!("rng: {e}")))?;

    let kek_secret = derive_kek(password, &salt)?;

    let (secret_bytes, public_key) = SESSION_KEY.with(|cell| {
        let borrow = cell.borrow();
        let key = borrow.as_ref().ok_or_else(|| {
            JsError::new("no session key loaded; call generateKey or unlockKeyBlob first")
        })?;
        Ok::<_, JsError>((key.to_bytes().to_vec(), key.public().to_bytes().to_vec()))
    })?;

    let encrypted_blob = kek_secret
        .encrypt(&secret_bytes)
        .map_err(|e| JsError::new(&format!("blob encrypt: {e}")))?;

    Ok(KeyBlob {
        encrypted_blob,
        salt: salt.to_vec(),
        public_key,
    })
}

/// Unlock a stored identity-vault blob with the viewer's password. On
/// success the recovered Ed25519 secret is loaded into the session and
/// [`decrypt_blob`] becomes usable for the `Sealed` envelope variant.
///
/// Errors on wrong password (AEAD auth-tag mismatch), malformed blob, or
/// KDF/length issues.
#[wasm_bindgen(js_name = unlockKeyBlob)]
pub fn unlock_key_blob(blob: &[u8], salt: &[u8], password: &str) -> Result<(), JsError> {
    let kek_secret = derive_kek(password, salt)?;
    let secret_bytes = kek_secret
        .decrypt(blob)
        .map_err(|_| JsError::new("unlock failed: wrong password or corrupt blob"))?;
    let arr: [u8; 32] = secret_bytes
        .as_slice()
        .try_into()
        .map_err(|_| JsError::new("unlocked plaintext is not 32 bytes"))?;
    let sk = PrivateKey::from(arr);
    SESSION_KEY.with(|s| *s.borrow_mut() = Some(sk));
    Ok(())
}
