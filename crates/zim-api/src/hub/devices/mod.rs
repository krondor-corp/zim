//! `/api/v0/devices/*` — the account's enrolled keys (web + daemons).

pub mod list;
pub mod remove;

pub use list::DevicesRequest;
pub use list::DevicesResponse;
pub use remove::RemoveDeviceRequest;

mod wire;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zim_crypto::PublicKey;
use zim_did::Did;

/// Device class. Same lowercase tags the hub stores (`web` | `daemon`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    /// Browser-resident key; reached via the hub (no iroh endpoint).
    Web,
    /// A `zim` daemon identity; dialed directly.
    Daemon,
}

impl DeviceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeviceKind::Web => "web",
            DeviceKind::Daemon => "daemon",
        }
    }
}

impl std::fmt::Display for DeviceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One device the hub reports for an account. Shared wire type: the hub
/// server serializes it, the daemon CLI and the web SPA deserialize it.
/// `PartialEq` so the web SPA can hold it in Yew state/props.
///
/// The JSON is unchanged from the old all-`String` shape: `pubkey` is
/// lowercase hex, `did` is `""` when absent, `created_at` is RFC 3339
/// (`""` when absent).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Device {
    #[serde(with = "wire::pubkey_hex")]
    pub pubkey: PublicKey,
    /// `did:key:z…` for the device. Older hubs send `""` (or omit it);
    /// that is `None`, and `None` goes back out as `""`.
    #[serde(default, with = "wire::empty_is_none")]
    pub did: Option<Did>,
    pub label: Option<String>,
    pub kind: DeviceKind,
    #[serde(default, with = "wire::timestamp")]
    pub created_at: Option<DateTime<Utc>>,
}

impl Device {
    /// The device's DID — the hub's `did` when sent, else derived from
    /// the key (they are the same `did:key`).
    pub fn did(&self) -> Did {
        self.did
            .clone()
            .unwrap_or_else(|| Did::from_key(&self.pubkey))
    }

    /// Stable address-book nick: its label if set, else
    /// `hub-<first 8 of pubkey hex>` so re-syncing is idempotent.
    pub fn nick(&self) -> String {
        match &self.label {
            Some(l) if !l.trim().is_empty() => l.trim().to_string(),
            _ => format!("hub-{}", &self.pubkey.to_hex()[..8]),
        }
    }
}

#[cfg(test)]
mod tests;
