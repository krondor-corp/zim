//! Local peer address book — `nick → Identity + bookkeeping`.
//!
//! [`PeerStore`] is the trait every binary plugs an impl into; same
//! shape as [`VaultLog`](crate::vault::VaultLog), and it lives here
//! for the same reason: it's a pure storage abstraction over
//! [`PublicKey`] / [`Identity`] with no transport coupling, so
//! browser/wasm clients can implement it against their own backends
//! just like native binaries do.
//!
//! Two concerns drive the abstraction:
//!
//! 1. The spam gate at the sync layer: before a fresh `HeadAdvanced`
//!    bootstraps a whole new vault, the coordinator asks
//!    [`PeerStore::knows`] whether we've ever seen the sender. Unknown
//!    senders are dropped.
//! 2. Different binaries persist this differently. The `zim` daemon
//!    keeps a `contacts` table in its `log.sqlite` (the
//!    `zim_peer::SqlitePeerStore`). Both it and the in-memory test
//!    impl implement this trait; the coordinator + HTTP layer don't
//!    care which one is in use.
//!
//! The in-memory impl backing unit tests is
//! `zim_peer::MemoryPeerStore`.

use std::fmt::{Debug, Display};

use async_trait::async_trait;
use zim_crypto::PublicKey;
use zim_did::Identity;

/// One address-book row. `nick` is the user-chosen handle; `identity`
/// is the peer's DID (carries the pubkey for `did:key`, the document
/// URL for `did:web`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerEntry {
    pub nick: String,
    pub identity: Identity,
    /// The relay host this contact is *reached through*, mirroring
    /// [`Share::via`](crate::fs::Share::via): `None` for a directly
    /// dialable peer (a daemon), `Some(host)` for a browser/web device
    /// reached via its hub. Distinct from `identity` ("who"), this is
    /// "where" — see the `recipient` vs `reach` split on `Share`.
    pub via: Option<Identity>,
    /// Whether this contact is *trusted*. Trusted contacts (your own
    /// devices, plus anyone you explicitly trust) are auto-propagated
    /// into the vaults you own; untrusted ones are shareable but opt-in
    /// per vault. See [`PeerStore::list_trusted`].
    pub trusted: bool,
    /// Unix epoch seconds. Set once on first insert; preserved across
    /// `upsert` re-adds so the user's original "added on" date sticks.
    pub added_at: i64,
    /// Free-form notes. Never inspected by the daemon; round-tripped
    /// for the user.
    pub notes: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PeerStoreError<E> {
    #[error("unknown peer: {0}")]
    NotFound(String),
    #[error("backend: {0}")]
    Backend(E),
}

#[async_trait]
pub trait PeerStore: Send + Sync + Debug + Clone + 'static {
    /// Error type for the underlying storage backend.
    type Error: Display + Debug + Send + Sync + 'static;

    /// Whether we have a contact whose identity resolves to `pubkey`.
    /// The spam gate at the sync layer calls this on every fresh
    /// `HeadAdvanced` — incoming connections are addressed by raw
    /// pubkey at the iroh layer, so the trait keeps a pubkey-shaped
    /// query even though storage is DID-shaped.
    ///
    /// Default impl: linear scan over [`list`](Self::list), comparing
    /// `entry.identity.pubkey()` against the query. Backends with a
    /// direct index (e.g. a SQL `WHERE pubkey = ?` lookup) can
    /// override.
    async fn knows(&self, pubkey: &PublicKey) -> Result<bool, PeerStoreError<Self::Error>> {
        let entries = self.list().await?;
        Ok(entries
            .into_iter()
            .any(|e| e.identity.pubkey() == Some(pubkey)))
    }

    /// All known peers in insertion order (or whatever the backend's
    /// natural order is — callers should not depend on this).
    async fn list(&self) -> Result<Vec<PeerEntry>, PeerStoreError<Self::Error>>;

    /// Look up by nick. `None` if no entry has that nick.
    async fn get(&self, nick: &str) -> Result<Option<PeerEntry>, PeerStoreError<Self::Error>>;

    /// Insert or replace. Existing `added_at` is preserved across
    /// re-adds; `notes` is preserved if `notes` here is `None`.
    /// `trusted` is set as given on every write.
    async fn upsert(
        &self,
        nick: &str,
        identity: Identity,
        trusted: bool,
        notes: Option<String>,
    ) -> Result<(), PeerStoreError<Self::Error>>;

    /// Remove by nick. Returns the removed entry, or
    /// `NotFound(nick)` if no entry has that nick.
    async fn remove(&self, nick: &str) -> Result<PeerEntry, PeerStoreError<Self::Error>>;

    /// Flip the `trusted` flag on an existing contact, preserving its
    /// identity, notes, and `added_at`. `NotFound(nick)` if absent.
    ///
    /// Default impl: `get` then `upsert`. Backends with a direct
    /// `UPDATE … SET trusted = ?` can override.
    async fn set_trusted(
        &self,
        nick: &str,
        trusted: bool,
    ) -> Result<(), PeerStoreError<Self::Error>> {
        let entry = self
            .get(nick)
            .await?
            .ok_or_else(|| PeerStoreError::NotFound(nick.to_string()))?;
        self.upsert(nick, entry.identity, trusted, entry.notes)
            .await
    }

    /// Every trusted contact. Default impl filters [`list`](Self::list);
    /// the reconcile sweep calls this to find the identities to fold
    /// into the owner's vaults.
    async fn list_trusted(&self) -> Result<Vec<PeerEntry>, PeerStoreError<Self::Error>> {
        Ok(self
            .list()
            .await?
            .into_iter()
            .filter(|e| e.trusted)
            .collect())
    }
}
