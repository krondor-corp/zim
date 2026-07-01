//! Typed hub HTTP client.
//!
//! Each request type documents the hub route it targets. The hub server
//! (`zim-hub`) implements those routes by hand in axum — there is no
//! compile-time link between the two, so **keep them in lockstep**: if
//! you change a path/shape here, change the matching `zim-hub` route.

pub mod jwt;

use serde::{Deserialize, Serialize};

#[cfg(feature = "client")]
use reqwest::{Client as HttpClient, RequestBuilder};
#[cfg(feature = "client")]
use url::Url;
#[cfg(feature = "client")]
use zim_crypto::PrivateKey;

#[cfg(feature = "client")]
use crate::{ApiError, ApiRequest, Client};

/// One device the hub reports for an account. Shared wire type: the hub
/// server serializes it, the daemon CLI and the web SPA deserialize it.
/// `PartialEq` so the web SPA can hold it in Yew state/props.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Device {
    pub pubkey: String,
    /// `did:key:z…` for the device. May be empty on older hubs; callers
    /// fall back to [`device_did`].
    #[serde(default)]
    pub did: String,
    pub label: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicesResponse {
    pub devices: Vec<Device>,
}

/// `GET /api/v0/devices` — the account's full device roster.
///
/// Auth is client-level (bearer for daemons, session cookie for the
/// browser), so this request carries none. **Hub route to mirror:**
/// `GET /api/v0/devices` (RequireUser).
#[cfg(feature = "client")]
pub struct DevicesRequest;

#[cfg(feature = "client")]
impl ApiRequest for DevicesRequest {
    type Response = DevicesResponse;
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.get(base.join("/api/v0/devices").expect("static path"))
    }
}

/// A hub's or user's DID document. We read only `id` here; full
/// verification-method resolution goes through `zim-did`.
#[derive(Debug, Deserialize)]
pub struct DidDoc {
    pub id: String,
}

/// `GET <path>` — fetch a DID document by absolute path (e.g.
/// `/.well-known/did.json` or `/u/<id>/did.json`). Public; no auth.
///
/// Hub routes: `GET /.well-known/did.json`, `GET /u/{user_id}/did.json`.
#[cfg(feature = "client")]
pub struct DidDocRequest {
    pub path: String,
}

#[cfg(feature = "client")]
impl ApiRequest for DidDocRequest {
    type Response = DidDoc;
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.get(base.join(&self.path).expect("caller passes a valid path"))
    }
}

/// Daemon-side hub client: a [`Client`] whose bearer is a JWT minted
/// from the daemon's identity key (`aud` = the hub URL). The browser
/// uses a plain cookie-authed [`Client`] against the same request types
/// instead of this.
#[cfg(feature = "client")]
pub struct HubClient {
    client: Client,
    self_pubkey_hex: String,
    hub_url: String,
}

#[cfg(feature = "client")]
impl HubClient {
    pub fn new(hub_url: &str, secret: PrivateKey) -> Result<Self, ApiError> {
        let base = Url::parse(hub_url)?;
        let hub_url = hub_url.trim_end_matches('/').to_string();
        // Mint once at construction — JWTs are short-lived but a CLI op
        // completes well inside the TTL.
        let token = jwt::mint(&secret, &hub_url);
        Ok(Self {
            client: Client::with_bearer(&base, &token)?,
            self_pubkey_hex: secret.public().to_hex(),
            hub_url,
        })
    }

    pub fn hub_url(&self) -> &str {
        &self.hub_url
    }

    /// This identity's pubkey hex.
    pub fn self_pubkey_hex(&self) -> String {
        self.self_pubkey_hex.clone()
    }

    /// `GET /api/v0/devices` — the account's device roster.
    pub async fn devices(&self) -> Result<Vec<Device>, ApiError> {
        Ok(self.client.call(DevicesRequest).await?.devices)
    }

    /// Fetch a DID document by absolute path (public, no auth).
    pub async fn did_doc(&self, path: &str) -> Result<DidDoc, ApiError> {
        self.client
            .call(DidDocRequest {
                path: path.to_string(),
            })
            .await
    }
}

/// `did:key:z…` for a device — its `did` field when present, else
/// synthesised from the pubkey hex. `None` if neither yields a key.
pub fn device_did(d: &Device) -> Option<String> {
    if !d.did.is_empty() {
        return Some(d.did.clone());
    }
    let pk = zim_crypto::PublicKey::from_hex(&d.pubkey).ok()?;
    Some(format!("did:key:{}", zim_did::did_key_encode(&pk)))
}

/// Stable address-book nick for a device: its label if set, else
/// `hub-<first 8 of pubkey>` so re-syncing is idempotent.
pub fn device_nick(d: &Device) -> String {
    match &d.label {
        Some(l) if !l.trim().is_empty() => l.trim().to_string(),
        _ => format!("hub-{}", &d.pubkey[..d.pubkey.len().min(8)]),
    }
}
