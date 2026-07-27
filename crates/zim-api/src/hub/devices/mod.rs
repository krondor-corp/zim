//! `/api/v0/devices/*` — the account's enrolled keys (web + daemons).

pub mod list;
pub mod remove;

pub use list::DevicesRequest;
pub use list::DevicesResponse;
pub use remove::RemoveDeviceRequest;

use serde::{Deserialize, Serialize};

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
