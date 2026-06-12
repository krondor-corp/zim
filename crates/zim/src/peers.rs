//! Local peer address book — nickname → pubkey + bookkeeping.
//!
//! Lives at `$ZIM_HOME/peers.toml`. Per-daemon (not shared in any
//! vault manifest), human-editable, owned by the daemon. The CLI
//! consults it via the `/api/v0/peers` HTTP endpoints; every place
//! that takes a pubkey (`share`, `unshare`, `relay`, `unrelay`,
//! `sync`) resolves a nickname here before sending the request.
//!
//! [`PeerBook`] is the on-disk shape. [`TomlPeerStore`] wraps it in a
//! `PathBuf` handle and implements [`zim_peer::PeerStore`] so the
//! sync coordinator can consult the book at runtime (e.g., to gate
//! incoming `ShareOffered` against known peers).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use zim_did::Identity;
use zim_peer::peers::{PeerEntry, PeerStore, PeerStoreError};

#[derive(Debug, thiserror::Error)]
pub enum PeersError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("toml: {0}")]
    Toml(String),
    #[error("unknown peer: {0}")]
    NotFound(String),
}

/// A single address-book entry. `nick` is the key in the book; the
/// rest is metadata.
///
/// `did` is the canonical identity URL (`did:key:z…` for daemons,
/// `did:web:hub.example.com:u:alice` for users hosted on a hub). We
/// keep it as a string on disk so the TOML file stays
/// human-editable; parsing happens at read time.
///
/// For backwards-compat reading of older books that used a `pubkey =
/// "<hex>"` field instead of `did`, [`Peer::resolve_did`] falls back
/// to a `did:key` synthesised from the hex pubkey. The fallback is
/// read-only — every new write goes out as `did = "…"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    /// DID URL. Optional only to keep deserialization of pre-DID
    /// books working — see [`Peer::resolve_did`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub did: Option<String>,
    /// Legacy hex pubkey from pre-DID books. Read-only fallback; new
    /// writes never emit this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pubkey: Option<String>,
    /// Unix epoch seconds.
    #[serde(default = "now_ts")]
    pub added_at: i64,
    /// Free-form notes — never inspected by the daemon, just round-
    /// tripped for the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Peer {
    /// Best-effort DID string for this entry: prefer the `did` field,
    /// fall back to synthesising `did:key:<pk>` from the legacy
    /// `pubkey` field. Returns `None` if neither field is set or the
    /// legacy hex doesn't decode.
    pub fn resolve_did(&self) -> Option<String> {
        if let Some(did) = &self.did {
            return Some(did.clone());
        }
        let hex = self.pubkey.as_deref()?;
        let pk = zim_crypto::PublicKey::from_hex(hex).ok()?;
        Some(format!("did:key:{}", zim_did::did_key_encode(&pk)))
    }
}

fn now_ts() -> i64 {
    Utc::now().timestamp()
}

/// On-disk shape: `[peers.<nick>] pubkey = "..."`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PeerBook {
    #[serde(default)]
    pub peers: BTreeMap<String, Peer>,
}

impl PeerBook {
    /// Load from `home/peers.toml`. Missing file → empty book.
    pub fn load(home: &Path) -> Result<Self, PeersError> {
        let path = crate::context::paths::peers_file(home);
        if !path.exists() {
            return Ok(Self::default());
        }
        let body = std::fs::read_to_string(&path)?;
        toml::from_str(&body).map_err(|e| PeersError::Toml(e.to_string()))
    }

    /// Serialize and write to `home/peers.toml`. Atomic-ish:
    /// write-then-rename so a crashing daemon doesn't leave a
    /// half-written book.
    pub fn save(&self, home: &Path) -> Result<(), PeersError> {
        let path = crate::context::paths::peers_file(home);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = toml::to_string_pretty(self).map_err(|e| PeersError::Toml(e.to_string()))?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, body)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Insert (or replace) an entry. `nick` keys the entry; `did`
    /// is the canonical DID URL. Existing notes and `added_at`
    /// survive a re-add. Always writes to the new `did` field; if
    /// the prior entry was a legacy `pubkey`-only row it gets
    /// migrated on this write.
    pub fn upsert(&mut self, nick: String, did: String, notes: Option<String>) {
        let prev = self.peers.get(&nick);
        let added_at = prev.map(|p| p.added_at).unwrap_or_else(now_ts);
        let notes = notes.or_else(|| prev.and_then(|p| p.notes.clone()));
        self.peers.insert(
            nick,
            Peer {
                did: Some(did),
                pubkey: None,
                added_at,
                notes,
            },
        );
    }

    /// Remove an entry by nick. Returns `Err(NotFound)` if missing —
    /// the HTTP layer maps that to a 404.
    pub fn remove(&mut self, nick: &str) -> Result<Peer, PeersError> {
        self.peers
            .remove(nick)
            .ok_or_else(|| PeersError::NotFound(nick.to_string()))
    }

    pub fn get(&self, nick: &str) -> Option<&Peer> {
        self.peers.get(nick)
    }

    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }
}

/// `$ZIM_HOME`-backed [`PeerStore`]. Loads + saves `peers.toml` on
/// every call — fits the existing HTTP-handler pattern and keeps the
/// human-editable file authoritative. Cheap because the book is tiny
/// (one entry per known peer, well under a kilobyte in practice).
#[derive(Debug, Clone)]
pub struct TomlPeerStore {
    home: PathBuf,
}

impl TomlPeerStore {
    pub fn new(home: PathBuf) -> Self {
        Self { home }
    }
}

#[async_trait]
impl PeerStore for TomlPeerStore {
    type Error = PeersError;

    async fn list(&self) -> Result<Vec<PeerEntry>, PeerStoreError<Self::Error>> {
        let book = PeerBook::load(&self.home).map_err(PeerStoreError::Backend)?;
        let mut out = Vec::with_capacity(book.peers.len());
        for (nick, p) in book.peers {
            out.push(entry_from(nick, p)?);
        }
        Ok(out)
    }

    async fn get(&self, nick: &str) -> Result<Option<PeerEntry>, PeerStoreError<Self::Error>> {
        let book = PeerBook::load(&self.home).map_err(PeerStoreError::Backend)?;
        book.peers
            .get(nick)
            .cloned()
            .map(|p| entry_from(nick.to_string(), p))
            .transpose()
    }

    async fn upsert(
        &self,
        nick: &str,
        identity: Identity,
        notes: Option<String>,
    ) -> Result<(), PeerStoreError<Self::Error>> {
        let mut book = PeerBook::load(&self.home).map_err(PeerStoreError::Backend)?;
        book.upsert(nick.to_string(), identity.to_string(), notes);
        book.save(&self.home).map_err(PeerStoreError::Backend)?;
        Ok(())
    }

    async fn remove(&self, nick: &str) -> Result<PeerEntry, PeerStoreError<Self::Error>> {
        let mut book = PeerBook::load(&self.home).map_err(PeerStoreError::Backend)?;
        let p = book
            .remove(nick)
            .map_err(|_| PeerStoreError::NotFound(nick.to_string()))?;
        book.save(&self.home).map_err(PeerStoreError::Backend)?;
        entry_from(nick.to_string(), p)
    }
}

fn entry_from(nick: String, p: Peer) -> Result<PeerEntry, PeerStoreError<PeersError>> {
    let did_string = p.resolve_did().ok_or_else(|| {
        PeerStoreError::Backend(PeersError::Toml(format!(
            "peer `{nick}` has no DID or pubkey field"
        )))
    })?;
    let identity = Identity::parse(&did_string).map_err(|e| {
        PeerStoreError::Backend(PeersError::Toml(format!("bad DID for `{nick}`: {e}")))
    })?;
    Ok(PeerEntry {
        nick,
        identity,
        added_at: p.added_at,
        notes: p.notes,
    })
}
