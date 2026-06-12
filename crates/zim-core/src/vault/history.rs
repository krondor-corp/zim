//! `Vault::history` — chain walk against the log.
//!
//! Plaintext metadata only: walks the [`VaultLog`] backward from a given
//! height and returns one `HistoryEntry` per step. Manifest decryption
//! is **not** required — relays, the browser, and the daemon all use
//! the same walker.

use serde::Serialize;

use crate::linked_data::Link;

/// One revision in the chain.
///
/// `previous` is `None` for the genesis entry. The relationship to
/// `link` is purely the canonical (lexicographically greatest) head at
/// each height — forks are resolved by the sync layer before history
/// is asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistoryEntry {
    pub height: u64,
    pub link: Link,
}
