//! Per-entry key schedule: a skip ratchet (WNFS lineage) that derives
//! each successive version's content [`Secret`] from the previous one,
//! plus the stable [`EntryId`] that names the entity across versions
//! and renames.
//!
//! Two chainings, kept separate on purpose:
//!
//! - **Spatial** (down the tree) is the existing Cryptree wrapping — a
//!   parent dir body holds its children's ratchets. Untouched here.
//! - **Temporal** (across revisions) is this module — one entity's key
//!   advances forward through its own versions: `key_{n+1}` is derived
//!   from `key_n`, never random. Holding revision `n` lets a reader
//!   derive `n, n+1, …` (grant-from-a-point) but not `n-1`.
//!
//! The ratchet is **content-independent** — a key clock, not bound to
//! blob hashes (binding it would kill skip-ahead). Versioning is the
//! parallel `previous` link on the entry. The writer mints/advances and
//! ships the ratchet *state* in the op, so every replaying peer
//! reconstructs the identical entry; peers never advance independently.

use rand_core::OsRng;
use serde::{Deserialize, Serialize};

use crate::Secret;

/// Domain separation for content keys derived from an entry ratchet.
/// Changing this string changes every derived key — it is part of the
/// on-disk format.
const CONTENT_KEY_DOMAIN: &str = "zim/entry-content/v1";

/// Stable identity of a tree entity, minted once at creation and
/// carried through every rewrite and rename. Random — provenance (who
/// created it, when) is recoverable from the creating op in the ops
/// log rather than encoded here. Not key material.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntryId([u8; 32]);

impl EntryId {
    /// Mint a fresh identity.
    pub fn generate() -> Self {
        let mut b = [0u8; 32];
        getrandom::getrandom(&mut b).expect("failed to generate random bytes");
        Self(b)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl std::fmt::Debug for EntryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EntryId({}…)", &self.to_hex()[..8])
    }
}

/// One entity's key schedule. Wraps the WNFS skip ratchet; the only
/// operations zim needs are *seed*, *advance*, and *derive this
/// revision's content key*.
///
/// Boxed: the ratchet state is ~135 bytes, and an `EntryRatchet` rides
/// inside every `Entry` (so every dir body) and every `AddFile`/`Mkdir`
/// op. Inline it dominated those enums (clippy `large_enum_variant`);
/// as a pointer it is 8 bytes everywhere, uniformly. Serde is
/// unaffected — a newtype over `Box<T>` serializes as `T`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryRatchet(Box<skip_ratchet::Ratchet>);

impl EntryRatchet {
    /// A fresh ratchet for a brand-new entity (revision 0).
    pub fn seed() -> Self {
        Self(Box::new(skip_ratchet::Ratchet::from_rng(&mut OsRng)))
    }

    /// The ratchet for the next revision. Pure and one-way: the result
    /// can derive every later key, but nothing can recover `self` from
    /// it.
    pub fn advanced(&self) -> Self {
        let mut next = (*self.0).clone();
        next.inc();
        Self(Box::new(next))
    }

    /// The content [`Secret`] for THIS revision. Deterministic — the
    /// same ratchet state always yields the same key, which is what lets
    /// a replaying peer rebuild the writer's exact entry.
    pub fn key(&self) -> Secret {
        let hash = self.0.derive_key(CONTENT_KEY_DOMAIN).finalize();
        Secret::from_slice(hash.as_bytes()).expect("blake3 output is exactly SECRET_SIZE bytes")
    }
}

impl std::fmt::Debug for EntryRatchet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print ratchet state — it is key material.
        f.write_str("EntryRatchet(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_state_derives_same_key_and_advance_changes_it() {
        // Alice seeds a ratchet for a new note.
        let r0 = EntryRatchet::seed();
        assert_eq!(r0.key(), r0.key(), "key derivation is deterministic");

        // She saves twice: each revision has its own key, none equal.
        let r1 = r0.advanced();
        let r2 = r1.advanced();
        assert_ne!(r0.key(), r1.key());
        assert_ne!(r1.key(), r2.key());
        assert_ne!(r0.key(), r2.key());

        // Advancing is pure — re-deriving from r0 lands on the same r1.
        assert_eq!(r0.advanced(), r1);
    }

    #[test]
    fn ratchet_survives_a_serde_round_trip_with_the_same_key() {
        // Bob receives Alice's op over the wire (CBOR via serde).
        let alice = EntryRatchet::seed().advanced();
        let bytes = serde_json::to_vec(&alice).unwrap();
        let bob: EntryRatchet = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(alice, bob);
        assert_eq!(
            alice.key(),
            bob.key(),
            "Bob derives Alice's exact content key"
        );
    }

    #[test]
    fn two_seeds_are_independent() {
        let a = EntryRatchet::seed();
        let b = EntryRatchet::seed();
        assert_ne!(a, b);
        assert_ne!(a.key(), b.key());
        assert_ne!(EntryId::generate(), EntryId::generate());
    }
}
