//! Lightweight typed client for the hub HTTP API.
//!
//! Every endpoint is a [`HubRequest`]: it owns its route (path + method +
//! body) in `build_request` and decodes its own response. [`call`] applies
//! auth, runs the exchange (reqwest's wasm client is `fetch`-backed and
//! `!Send`, so inside a `SendWrapper` to keep the outer future `Send`), maps
//! non-2xx to `Err`, and decodes. Both the [`crate::fs`] vault stores and the
//! JS-facing [`HubClient`] dispatch through `call`, so routing + auth + error
//! handling live in exactly one place.
//!
//! Two auth modes, because the hub's `RequireUser` accepts either:
//! - [`Auth::Bearer`] — a short-lived EdDSA JWT minted from the session key,
//!   for vault endpoints (the key is an enrolled peer).
//! - [`Auth::Cookie`] — the browser session cookie (sent same-origin), for
//!   escrow + device enrollment, where the key isn't a peer yet.

use base64::engine::general_purpose::{STANDARD as B64, URL_SAFE_NO_PAD};
use base64::Engine;
use bytes::Bytes;
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use url::Url;
use wasm_bindgen::prelude::*;

use zim_core::linked_data::{Hash, Link};
use zim_core::vault::VaultId;
use zim_crypto::PublicKey;

use crate::SESSION_KEY;

/// Server-opaque label describing the escrow wrap scheme (see
/// [`crate::encrypt_key_blob`]).
pub(crate) const KDF_LABEL: &str = "argon2id-19456-2-1+chacha20poly1305";

// ---------------------------------------------------------------------------
// Auth + dispatch.
// ---------------------------------------------------------------------------

/// Mint an EdDSA JWT for `base` (the audience) from the loaded session key.
/// Format matches `zim-hub`'s verifier: `kid`/`iss` are the pubkey hex.
fn mint_jwt(base: &Url) -> Result<String, String> {
    SESSION_KEY.with(|cell| {
        let borrow = cell.borrow();
        let sk = borrow.as_ref().ok_or("no session key loaded")?;
        let kid = sk.public().to_hex();
        let now = (js_sys::Date::now() / 1000.0) as i64;
        let aud = base.as_str().trim_end_matches('/');
        let header =
            URL_SAFE_NO_PAD.encode(format!(r#"{{"alg":"EdDSA","typ":"JWT","kid":"{kid}"}}"#));
        let payload = URL_SAFE_NO_PAD.encode(format!(
            r#"{{"iss":"{kid}","aud":"{aud}","iat":{now},"exp":{}}}"#,
            now + 60
        ));
        let sig =
            URL_SAFE_NO_PAD.encode(sk.sign(format!("{header}.{payload}").as_bytes()).to_bytes());
        Ok(format!("{header}.{payload}.{sig}"))
    })
}

pub(crate) enum Auth {
    /// Bearer JWT minted from the session key.
    Bearer,
    /// Browser session cookie, sent same-origin.
    Cookie,
}

pub(crate) trait HubRequest {
    type Response;
    const AUTH: Auth;
    /// Build the route: path + method + body. Auth is applied by [`call`].
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder;
    /// Decode the 2xx response body.
    fn decode(bytes: Bytes) -> Result<Self::Response, String>;
}

pub(crate) async fn call<R: HubRequest>(
    base: &Url,
    client: &Client,
    req: R,
) -> Result<R::Response, String> {
    let builder = req.build_request(base, client);
    let builder = match R::AUTH {
        Auth::Bearer => builder.bearer_auth(mint_jwt(base)?),
        Auth::Cookie => builder.fetch_credentials_same_origin(),
    };
    let bytes = send_wrapper::SendWrapper::new(async move {
        let resp = builder.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        if status.is_success() {
            Ok(bytes)
        } else {
            Err(format!(
                "HTTP {status}: {}",
                String::from_utf8_lossy(&bytes)
            ))
        }
    })
    .await?;
    R::decode(bytes)
}

/// `base` joined with an absolute hub path. Panics only on a malformed base,
/// which [`HubClient::new`] / `parse_base` already reject.
fn route(base: &Url, path: &str) -> Url {
    base.join(path).expect("valid hub route")
}

fn from_json<T: serde::de::DeserializeOwned>(bytes: Bytes) -> Result<T, String> {
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn no_body(_bytes: Bytes) -> Result<(), String> {
    Ok(())
}

// ---------------------------------------------------------------------------
// Vault routes (Bearer). Mirror `zim_hub::http::api::v0`.
// ---------------------------------------------------------------------------

/// `GET /api/v0/blob/{hash}` — fetch ciphertext (raw body).
pub(crate) struct GetBlob(pub Hash);
impl HubRequest for GetBlob {
    type Response = Bytes;
    const AUTH: Auth = Auth::Bearer;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client.get(route(base, &format!("/api/v0/blob/{}", self.0.to_hex())))
    }
    fn decode(bytes: Bytes) -> Result<Bytes, String> {
        Ok(bytes)
    }
}

/// `PUT /api/v0/blob` — store ciphertext, returns its hash.
pub(crate) struct PutBlob(pub Vec<u8>);
impl HubRequest for PutBlob {
    type Response = WriteBlobResponse;
    const AUTH: Auth = Auth::Bearer;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client.put(route(base, "/api/v0/blob")).body(self.0)
    }
    fn decode(bytes: Bytes) -> Result<WriteBlobResponse, String> {
        from_json(bytes)
    }
}

/// `GET /api/v0/v/{id}/head` — current canonical head + height.
pub(crate) struct GetHead(pub VaultId);
impl HubRequest for GetHead {
    type Response = HeadResponse;
    const AUTH: Auth = Auth::Bearer;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client.get(route(base, &format!("/api/v0/v/{}/head", self.0)))
    }
    fn decode(bytes: Bytes) -> Result<HeadResponse, String> {
        from_json(bytes)
    }
}

/// `POST /api/v0/v/{id}/head` — advance the head to a new manifest.
pub(crate) struct PostHead {
    pub id: VaultId,
    pub manifest_hash: String,
}
impl HubRequest for PostHead {
    type Response = ();
    const AUTH: Auth = Auth::Bearer;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client
            .post(route(base, &format!("/api/v0/v/{}/head", self.id)))
            .json(&WriteHeadRequest {
                manifest_hash: self.manifest_hash,
            })
    }
    fn decode(bytes: Bytes) -> Result<(), String> {
        no_body(bytes)
    }
}

/// `GET /api/v0/v/{id}/log?from=&limit=` — paginated chain walk.
pub(crate) struct GetLog {
    pub id: VaultId,
    pub from: u64,
    pub limit: u64,
}
impl HubRequest for GetLog {
    type Response = LogResponse;
    const AUTH: Auth = Auth::Bearer;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client.get(route(
            base,
            &format!(
                "/api/v0/v/{}/log?from={}&limit={}",
                self.id, self.from, self.limit
            ),
        ))
    }
    fn decode(bytes: Bytes) -> Result<LogResponse, String> {
        from_json(bytes)
    }
}

// ---------------------------------------------------------------------------
// Escrow + device enrollment routes (Cookie).
// ---------------------------------------------------------------------------

/// `PUT /api/v0/escrow` — store the passphrase-wrapped device key.
#[derive(Serialize)]
struct PutEscrow {
    did: String,
    salt: String,
    kdf: String,
    wrapped_secret: String,
    created_at: String,
}
impl HubRequest for PutEscrow {
    type Response = ();
    const AUTH: Auth = Auth::Cookie;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client.put(route(base, "/api/v0/escrow")).json(&self)
    }
    fn decode(bytes: Bytes) -> Result<(), String> {
        no_body(bytes)
    }
}

/// `GET /api/v0/devices/enroll-challenge` — issue a possession-proof challenge.
struct GetEnrollChallenge;
impl HubRequest for GetEnrollChallenge {
    type Response = ChallengeResponse;
    const AUTH: Auth = Auth::Cookie;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client.get(route(base, "/api/v0/devices/enroll-challenge"))
    }
    fn decode(bytes: Bytes) -> Result<ChallengeResponse, String> {
        from_json(bytes)
    }
}

/// `POST /api/v0/devices/self` — enroll this key with a signed challenge.
#[derive(Serialize)]
struct SelfEnroll {
    pubkey: String,
    label: String,
    kind: String,
    challenge: String,
    signature: String,
}
impl HubRequest for SelfEnroll {
    type Response = ();
    const AUTH: Auth = Auth::Cookie;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client.post(route(base, "/api/v0/devices/self")).json(&self)
    }
    fn decode(bytes: Bytes) -> Result<(), String> {
        no_body(bytes)
    }
}

/// `GET /u/{user_id}/did.json` — the user's did:web document (public; lists
/// every enrolled key). Cookie auth is harmless on a public route.
struct GetUserDid {
    user_id: String,
}
impl HubRequest for GetUserDid {
    type Response = DidDoc;
    const AUTH: Auth = Auth::Cookie;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client.get(route(base, &format!("/u/{}/did.json", self.user_id)))
    }
    fn decode(bytes: Bytes) -> Result<DidDoc, String> {
        from_json(bytes)
    }
}

#[derive(Deserialize)]
struct DidDoc {
    #[serde(rename = "verificationMethod")]
    verification_method: Vec<DidVerificationMethod>,
}

#[derive(Deserialize)]
struct DidVerificationMethod {
    #[serde(rename = "publicKeyMultibase")]
    public_key_multibase: String,
}

/// Resolve a user's did:web document to the full set of pubkeys enrolled to
/// them (web key + daemons) — used to seal a new vault to every device. The
/// `publicKeyMultibase` values are `did:key` `z…` identifiers.
pub(crate) async fn resolve_user_keys(base: &Url, user_id: &str) -> Result<Vec<PublicKey>, String> {
    let client = Client::new();
    let doc = call(
        base,
        &client,
        GetUserDid {
            user_id: user_id.to_string(),
        },
    )
    .await?;
    let total = doc.verification_method.len();
    let keys: Vec<PublicKey> = doc
        .verification_method
        .into_iter()
        .filter_map(|vm| zim_did::did_key_decode(&vm.public_key_multibase).ok())
        .collect();
    web_sys::console::log_1(
        &format!(
            "resolve_user_keys: did.json had {total} verification method(s), {} decoded ok",
            keys.len()
        )
        .into(),
    );
    Ok(keys)
}

/// `GET /.well-known/did.json` — the hub's own did:web document. Its single
/// verification method is the hub's iroh pubkey: the network identity peers
/// dial it on. A browser key is reachable only *through* the hub, so a vault
/// a browser creates seals its owner share with this key as the dial host.
struct GetHubDid;
impl HubRequest for GetHubDid {
    type Response = DidDoc;
    const AUTH: Auth = Auth::Cookie;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client.get(route(base, "/.well-known/did.json"))
    }
    fn decode(bytes: Bytes) -> Result<DidDoc, String> {
        from_json(bytes)
    }
}

/// Resolve the hub's own iroh pubkey from `/.well-known/did.json`. This is the
/// `via` host a browser-owned vault stamps on its owner share so a peer
/// advancing the vault dials the hub (which mirrors the head) rather than
/// trying — and failing — to dial the browser directly.
pub(crate) async fn resolve_hub_key(base: &Url) -> Result<PublicKey, String> {
    let client = Client::new();
    let doc = call(base, &client, GetHubDid).await?;
    doc.verification_method
        .into_iter()
        .find_map(|vm| zim_did::did_key_decode(&vm.public_key_multibase).ok())
        .ok_or_else(|| "hub did.json had no decodable verification method".to_string())
}

/// `GET /api/v0/escrow/list` — this user's escrowed key fragments.
struct GetEscrowList;
impl HubRequest for GetEscrowList {
    type Response = Vec<EscrowListItem>;
    const AUTH: Auth = Auth::Cookie;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client.get(route(base, "/api/v0/escrow/list"))
    }
    fn decode(bytes: Bytes) -> Result<Vec<EscrowListItem>, String> {
        from_json(bytes)
    }
}

/// `GET /api/v0/escrow?did=…` — the wrapped blob for one fragment.
struct GetEscrow {
    did: String,
}
impl HubRequest for GetEscrow {
    type Response = EscrowBlob;
    const AUTH: Auth = Auth::Cookie;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        let mut url = route(base, "/api/v0/escrow");
        url.query_pairs_mut().append_pair("did", &self.did);
        client.get(url)
    }
    fn decode(bytes: Bytes) -> Result<EscrowBlob, String> {
        from_json(bytes)
    }
}

// ---------------------------------------------------------------------------
// Wire types. Mirror the axum handlers' request/response structs.
// ---------------------------------------------------------------------------

/// `api::v0::blob::WriteBlobResponse`.
#[derive(Deserialize)]
pub(crate) struct WriteBlobResponse {
    pub hash: String,
}

/// `api::v0::vault::head::HeadResponse`.
#[derive(Deserialize)]
pub(crate) struct HeadResponse {
    pub link: Link,
    pub height: u64,
}

/// `api::v0::vault::log::LogResponse`.
#[derive(Deserialize)]
pub(crate) struct LogResponse {
    pub entries: Vec<LogEntry>,
}

#[derive(Deserialize)]
pub(crate) struct LogEntry {
    pub height: u64,
    pub link: Link,
}

/// `api::v0::vault::write_head::WriteHeadRequest`.
#[derive(Serialize)]
struct WriteHeadRequest {
    manifest_hash: String,
}

/// `api::v0::devices::ChallengeResponse`.
#[derive(Deserialize)]
struct ChallengeResponse {
    challenge: String,
}

/// `api::v0::escrow::EscrowListItem` (only the field we use).
#[derive(Deserialize)]
struct EscrowListItem {
    did: String,
}

/// `api::v0::escrow::EscrowBlob` (only the fields we use; serde drops the rest).
#[derive(Deserialize)]
struct EscrowBlob {
    salt: String,
    wrapped_secret: String,
}

// ---------------------------------------------------------------------------
// HubClient — the JS-facing SDK over the request layer.
// ---------------------------------------------------------------------------

#[wasm_bindgen]
pub struct HubClient {
    base: Url,
    client: Client,
}

#[wasm_bindgen]
impl HubClient {
    #[wasm_bindgen(constructor)]
    pub fn new(hub_base: String) -> Result<HubClient, JsError> {
        let base = hub_base
            .parse()
            .map_err(|e| JsError::new(&format!("invalid hub url: {e}")))?;
        Ok(Self {
            base,
            client: Client::new(),
        })
    }

    /// Enroll this browser as a device — all crypto + HTTP in WASM. Generates
    /// an ed25519 key, wraps the secret under `passphrase` (Argon2id +
    /// ChaCha20-Poly1305), escrows the wrapped blob, then proves possession
    /// against a fresh challenge. Returns the bits JS persists locally (the
    /// unlocked seed for the tab cache + the encrypted blob for IndexedDB).
    pub async fn enroll_browser_device(
        &self,
        did_fragment: String,
        passphrase: String,
    ) -> Result<LocalKey, JsError> {
        // 1. Fresh key into the session.
        let pubkey_hex = hex::encode(crate::generate_key());

        // 2. Wrap it under the passphrase.
        let blob = crate::encrypt_key_blob(&passphrase)?;
        let salt = blob.salt();
        let wrapped = blob.encrypted_blob();

        // 3. Escrow the wrapped blob (cookie-authed — key isn't a peer yet).
        call(
            &self.base,
            &self.client,
            PutEscrow {
                did: did_fragment.clone(),
                salt: B64.encode(&salt),
                kdf: KDF_LABEL.to_string(),
                wrapped_secret: B64.encode(&wrapped),
                created_at: String::new(),
            },
        )
        .await
        .map_err(|e| JsError::new(&e))?;

        // 4. Possession proof: challenge → sign `challenge || pubkey`.
        let challenge = call(&self.base, &self.client, GetEnrollChallenge)
            .await
            .map_err(|e| JsError::new(&e))?
            .challenge;
        let signature = crate::sign_enroll_challenge(&challenge)?;

        // 5. Enroll.
        call(
            &self.base,
            &self.client,
            SelfEnroll {
                pubkey: pubkey_hex,
                label: String::new(), // web key = account master identity, no label
                kind: "web".to_string(),
                challenge,
                signature,
            },
        )
        .await
        .map_err(|e| JsError::new(&e))?;

        Ok(LocalKey {
            did: did_fragment,
            seed_hex: crate::session_seed_hex()?,
            salt,
            wrapped,
        })
    }

    /// Recover the device key on a fresh browser: fetch the escrowed blob,
    /// unwrap it under `passphrase` (loading the secret into the session),
    /// and hand back the bits to persist locally. Mirrors enrollment's
    /// return so the caller caches the same way.
    pub async fn unlock_from_escrow(&self, passphrase: String) -> Result<LocalKey, JsError> {
        let did = call(&self.base, &self.client, GetEscrowList)
            .await
            .map_err(|e| JsError::new(&e))?
            .into_iter()
            .next()
            .map(|item| item.did)
            .ok_or_else(|| JsError::new("no escrowed key for this account"))?;

        let blob = call(&self.base, &self.client, GetEscrow { did: did.clone() })
            .await
            .map_err(|e| JsError::new(&e))?;
        let salt = B64
            .decode(&blob.salt)
            .map_err(|e| JsError::new(&format!("bad salt: {e}")))?;
        let wrapped = B64
            .decode(&blob.wrapped_secret)
            .map_err(|e| JsError::new(&format!("bad wrapped_secret: {e}")))?;

        // Loads the recovered secret into SESSION_KEY (or errors on a bad
        // passphrase / corrupt blob).
        crate::unlock_key_blob(&wrapped, &salt, &passphrase)?;

        Ok(LocalKey {
            did,
            seed_hex: crate::session_seed_hex()?,
            salt,
            wrapped,
        })
    }
}

/// The bits a caller persists for a device key: the unlocked seed (tab
/// cache) + the encrypted blob (IndexedDB). Returned by both
/// [`HubClient::enroll_browser_device`] and [`HubClient::unlock_from_escrow`].
#[wasm_bindgen]
pub struct LocalKey {
    did: String,
    seed_hex: String,
    salt: Vec<u8>,
    wrapped: Vec<u8>,
}

#[wasm_bindgen]
impl LocalKey {
    #[wasm_bindgen(getter)]
    pub fn did(&self) -> String {
        self.did.clone()
    }
    /// Hex of the unlocked seed — cache in `sessionStorage` for the tab.
    #[wasm_bindgen(getter)]
    pub fn seed_hex(&self) -> String {
        self.seed_hex.clone()
    }
    /// Argon2 salt — persist with the wrapped blob in IndexedDB.
    #[wasm_bindgen(getter)]
    pub fn salt(&self) -> Vec<u8> {
        self.salt.clone()
    }
    /// Encrypted key blob — persist at rest in IndexedDB.
    #[wasm_bindgen(getter)]
    pub fn wrapped(&self) -> Vec<u8> {
        self.wrapped.clone()
    }
}
