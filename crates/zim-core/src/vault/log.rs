//! Vault version log — the append-only chain of manifest versions.
//!
//! Every vault maintains an ordered history of manifest links. Each entry
//! records the link (content-addressed pointer to the manifest blob), its
//! height in the chain, and a back-pointer to the previous entry.
//!
//! The [`VaultLog`] trait abstracts the storage backend. Implementers only
//! need to provide the five core operations; the default [`VaultLog::head`]
//! method derives the canonical head from `heads` + `height`.
//!
//! Concrete implementations live one level up — `SqliteVaultLog` and
//! `MemoryVaultLog` in `zim-peer`. They impl this trait so the daemon's
//! disk-backed log and tests' in-memory log are interchangeable.

use std::fmt::{Debug, Display};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::id::VaultId;
use crate::linked_data::Link;

/// One position in a vault's version chain: the manifest link and its
/// height. This is THE currency of the sync protocol — log queries
/// return it, probes carry samples of it, peers announce it.
///
/// A named struct rather than `(Link, u64)` so field order can't
/// silently flip between call sites (probe samples used to be
/// `(height, link)` while everything else was `(link, height)`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Head {
    pub link: Link,
    pub height: u64,
}

impl Head {
    pub fn new(link: Link, height: u64) -> Self {
        Self { link, height }
    }
}

impl Display for Head {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.link, self.height)
    }
}

/// Errors that can occur when operating on a vault log.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum VaultLogError<T> {
    /// The underlying storage backend returned an error.
    #[error("provider error: {0}")]
    Provider(#[from] T),

    /// No head entry exists at the requested height.
    #[error("head not found at height {0}")]
    HeadNotFound(u64),

    /// The append would create a duplicate entry at the same height
    /// with the same link, indicating a replay or race.
    #[error("conflict with current log entry")]
    Conflict,

    /// The append's `previous` link does not match any entry at
    /// `height - 1`, so the chain is broken.
    #[error("invalid append: current={0}, previous={1}, height={2}")]
    InvalidAppend(Link, Link, u64),
}

/// Append-only version log for a single vault.
///
/// Each vault's history is a chain of (link, height) pairs. The log
/// supports forks (multiple heads at the same height) which are resolved
/// by the sync layer.
///
/// Implementers must provide the five required methods. The `head`,
/// `probe`, and `exponential_sample` methods have default implementations
/// in terms of the required ones; backends with single-shot query support
/// (e.g. SQLite `WHERE link IN (…)`) can override `probe` for a faster
/// path.
#[async_trait]
pub trait VaultLog: Send + Sync + Debug + Clone + 'static {
    /// Error type for the underlying storage backend.
    type Error: Display + Debug;

    /// Check whether any log entries exist for this vault.
    async fn exists(&self, id: VaultId) -> Result<bool, VaultLogError<Self::Error>>;

    /// Return all links recorded at `height`.
    ///
    /// A well-behaved log has exactly one link per height, but forks
    /// (from concurrent syncs) can produce multiple. The sync layer
    /// resolves these via merge.
    async fn heads(
        &self,
        id: VaultId,
        height: u64,
    ) -> Result<Vec<Link>, VaultLogError<Self::Error>>;

    /// Append a new version to the log.
    ///
    /// Fails with `Conflict` if an entry with the same link already
    /// exists at this height, or `InvalidAppend` if `previous` does
    /// not match any entry at `height - 1`.
    async fn append(
        &self,
        id: VaultId,
        name: String,
        current: Link,
        previous: Option<Link>,
        height: u64,
    ) -> Result<(), VaultLogError<Self::Error>>;

    /// Return the greatest height that has any entries.
    async fn height(&self, id: VaultId) -> Result<u64, VaultLogError<Self::Error>>;

    /// Return all heights at which `link` appears.
    ///
    /// Used during sync to find common ancestors between two peers'
    /// chains.
    async fn has(&self, id: VaultId, link: Link) -> Result<Vec<u64>, VaultLogError<Self::Error>>;

    /// Return the canonical head: the lexicographically greatest link
    /// at the given height (or the max height if `None`).
    ///
    /// This gives deterministic tie-breaking when multiple links exist
    /// at the same height.
    async fn head(
        &self,
        id: VaultId,
        height: Option<u64>,
    ) -> Result<Head, VaultLogError<Self::Error>> {
        let height = height.unwrap_or(self.height(id).await?);
        let heads = self.heads(id, height).await?;
        let link = heads
            .into_iter()
            .max()
            .ok_or(VaultLogError::HeadNotFound(height))?;
        Ok(Head::new(link, height))
    }

    /// Given a peer's sample of their chain (descending by height),
    /// return the deepest entry we also have in our log.
    ///
    /// This is the responder side of an ancestor probe: the initiator
    /// builds an [`Self::exponential_sample`] of their chain and sends
    /// it; we scan it and return the first match.
    ///
    /// Default implementation calls `has` once per sample entry. Backends
    /// with a single-shot `WHERE link IN (...)` query (e.g. SQLite) can
    /// override for a faster path.
    async fn probe(
        &self,
        id: VaultId,
        sample: &[Head],
    ) -> Result<Option<Head>, VaultLogError<Self::Error>> {
        for head in sample {
            let heights = self.has(id, head.link.clone()).await?;
            if !heights.is_empty() {
                return Ok(Some(head.clone()));
            }
        }
        Ok(None)
    }

    /// Build a git-style exponentially-spaced sample of our chain ending
    /// at the head: `[h, h-1, h-2, h-4, h-8, …, 0]`.
    ///
    /// Used as the payload for a `ProbeRequest`. One round-trip covers
    /// divergence up to `2^sample.len()` versions; for a 100-version
    /// chain the sample has 9 entries.
    ///
    /// Returns the empty vec if the vault has no log entries.
    async fn exponential_sample(
        &self,
        id: VaultId,
    ) -> Result<Vec<Head>, VaultLogError<Self::Error>> {
        if !self.exists(id).await? {
            return Ok(Vec::new());
        }
        let head_height = self.height(id).await?;

        let mut heights: Vec<u64> = vec![head_height];
        let mut delta: u64 = 1;
        while delta <= head_height {
            heights.push(head_height - delta);
            let Some(next) = delta.checked_mul(2) else {
                break;
            };
            delta = next;
        }
        if *heights.last().unwrap() != 0 {
            heights.push(0);
        }
        heights.dedup();

        let mut out = Vec::with_capacity(heights.len());
        for h in heights {
            let heads = self.heads(id, h).await?;
            if let Some(link) = heads.into_iter().max() {
                out.push(Head::new(link, h));
            }
        }
        Ok(out)
    }

    /// List all vault IDs that have at least one log entry.
    async fn list_vaults(&self) -> Result<Vec<VaultId>, VaultLogError<Self::Error>>;
}
