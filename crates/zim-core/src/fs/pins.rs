//! The pinned-blob set: hashes the vault wants to keep alive in the
//! inner blob store.
//!
//! Pins guard against inner-store GC. The set tracks everything that
//! lives *outside* the manifest blob — file content, the ops-log
//! ciphertext, the previous-manifest link — so a GC pass over the inner
//! store knows what to keep. Dir bodies are not pinned (they ship inline
//! inside the manifest's metadata pack).

use std::collections::BTreeSet;
use std::ops::Deref;

use serde::{Deserialize, Serialize};

use crate::linked_data::Hash;

/// The set of blob hashes the vault depends on. Serialized inside the
/// manifest. Mutated when files are added (insert), removed (remove on
/// the file's hash), or overwritten (remove old, insert new).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pins(BTreeSet<Hash>);

impl Deref for Pins {
    type Target = BTreeSet<Hash>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Default for Pins {
    fn default() -> Self {
        Self::new()
    }
}

impl Pins {
    /// Create an empty pin set.
    pub fn new() -> Self {
        Pins(BTreeSet::new())
    }

    /// Insert `hash` into the set. Returns whether the value was newly
    /// pinned (`true`) or already present (`false`).
    pub fn insert(&mut self, hash: Hash) -> bool {
        self.0.insert(hash)
    }

    /// Remove `hash` from the set. Returns whether it was present.
    /// Called when an entry referencing the hash is overwritten so the
    /// underlying blob becomes a candidate for inner-store GC.
    pub fn remove(&mut self, hash: &Hash) -> bool {
        self.0.remove(hash)
    }

    /// Insert every hash from `hashes`.
    pub fn extend<I>(&mut self, hashes: I)
    where
        I: IntoIterator<Item = Hash>,
    {
        self.0.extend(hashes)
    }

    /// Number of pinned hashes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True if no hashes are pinned.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// True if `hash` is currently pinned.
    pub fn contains(&self, hash: &Hash) -> bool {
        self.0.contains(hash)
    }

    /// Materialize the set as a `Vec` (e.g. for serialization that needs
    /// an ordered sequence).
    pub fn to_vec(&self) -> Vec<Hash> {
        self.0.iter().copied().collect()
    }

    /// Build a [`Pins`] from a `Vec<Hash>` (the deserialization
    /// counterpart of [`Self::to_vec`]).
    pub fn from_vec(hashes: Vec<Hash>) -> Self {
        Pins(hashes.into_iter().collect())
    }

    /// Iterate over the pinned hashes.
    pub fn iter(&self) -> impl Iterator<Item = &Hash> {
        self.0.iter()
    }
}
