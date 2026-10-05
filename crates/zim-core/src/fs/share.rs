//! [`Share`] — a peer's sealed handle to the vault's root ratchet,
//! stored on the [`Manifest`](super::Manifest) in a [`Shares`] map.
//!
//! A share carries an [`Identity`] (`did:key` or `did:web`) rather than
//! a raw pubkey — see `zim-did`. It also carries an optional `via`: the
//! always-on host a hosted client is reached *through*. This is what
//! folds the old separate `Relay` type away — a relay is just a share
//! with a `via` set (see the hosted-DID protocol in
//! `docs/product/identity.md`):
//!
//! - **`via = None`** — direct: the client is dialed as an iroh peer
//!   (its `NodeId` is the key). `did:key` shares.
//! - **`via = Some(host)`** — hosted: the secret is still sealed to the
//!   client (zero-knowledge — the host never holds it), but sync dials
//!   the host, never the client. The host is recorded as a resolved
//!   `did:key`, so routing/access checks stay synchronous.
//!
//! The caller (which owns a DID resolver) expands a `did:web` into one
//! share per verification method at share time via
//! [`zim_did::resolve_reaches`], sealing each client and stamping the
//! shared `via`. Every `Share` persisted on disk therefore carries a
//! concrete `did:key` for both `identity` and `via`.
//!
//! # Key material
//!
//! A share always holds key material: the root ratchet **state** sealed
//! to the recipient, stamped with the height that state derives
//! (`sealed_at`). A holder brings it forward by `height − sealed_at`
//! and derives any later root key itself, so saves never touch shares.
//! A share is sealed the moment it is granted, with the live root
//! state; revocation re-seeds the root and re-seals everyone remaining
//! on the spot. There is no "granted but not yet sealed" state.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use zim_crypto::{EntryRatchet, PrivateKey, PublicKey, RatchetShare, RatchetShareError};
use zim_did::Did;

/// Errors recovering the root ratchet from a share.
#[derive(Debug, thiserror::Error)]
pub enum ShareError {
    /// The key has no share on this manifest — not a member.
    #[error("no share for this key")]
    NotFound,
    /// A share is sealed at or before the height of the manifest it
    /// sits in. A newer `sealed_at` means the manifest is corrupt;
    /// deriving a key from it would only fail later with an opaque
    /// decrypt error.
    #[error("share sealed at height {sealed_at} is newer than manifest height {height}")]
    SealedAfterManifest { sealed_at: u64, height: u64 },
    /// Sealing to, or unsealing with, the recipient's key failed.
    #[error("ratchet share: {0}")]
    Crypto(#[from] RatchetShareError),
}

/// A peer's share of vault access.
///
/// Pairs a [`Did`] (the seal target) with a [`RatchetShare`] (the root
/// ratchet state sealed to that identity's pubkey) and an optional
/// `via` host the client is reached through.
///
/// Dialability is derived, never stored as a flag: a share with
/// `via = None` is dialed directly; one with `via = Some(host)` is
/// reached through `host`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Share {
    identity: Did,
    /// The root ratchet state sealed to `identity`, with the height it
    /// derives.
    ratchet_share: RatchetShare,
    /// The always-on host this client is reached through. `None` for a
    /// directly-dialable peer; `Some(did:key of host)` for a hosted
    /// client (e.g. a browser reached via the hub). The host never holds
    /// the vault secret — `ratchet_share` is sealed to the client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    via: Option<Did>,
}

impl Share {
    /// Construct a share from an already-sealed ratchet state.
    ///
    /// - `ratchet_share` — the root ratchet state encrypted to
    ///   `identity`'s underlying pubkey.
    /// - `identity` — the peer's DID-shaped identity (the seal target).
    /// - `via` — the host the client is reached through, or `None` for a
    ///   directly-dialable peer.
    pub fn new(ratchet_share: RatchetShare, identity: Did, via: Option<Did>) -> Self {
        Self {
            identity,
            ratchet_share,
            via,
        }
    }

    /// Seal `root` — the root ratchet state at `sealed_at` — to
    /// `recipient`, reached through `via`.
    pub fn seal(
        root: &EntryRatchet,
        recipient: &PublicKey,
        sealed_at: u64,
        via: Option<Did>,
    ) -> Result<Self, ShareError> {
        let ratchet_share = RatchetShare::new(root, recipient, sealed_at)?;
        Ok(Self::new(ratchet_share, Did::from_key(recipient), via))
    }

    /// Replace the sealed state with `root` at `sealed_at`, keeping
    /// identity and `via`. Used when the root lineage is re-seeded.
    pub fn reseal(
        &mut self,
        root: &EntryRatchet,
        recipient: &PublicKey,
        sealed_at: u64,
    ) -> Result<(), ShareError> {
        self.ratchet_share = RatchetShare::new(root, recipient, sealed_at)?;
        Ok(())
    }

    // Two distinct questions a share answers — keep them apart:
    //   * `identity` / `recipient` — *who* gets the secret (who decrypts).
    //   * `via` / `reach`          — *where* we dial or fetch for them.
    // They coincide for a directly-dialable peer and diverge for a hosted
    // one (a browser whose data lives on the hub).

    /// The peer's logical identity (DID) — the seal target. This is *who*
    /// the share is for, not where to reach them; see [`Self::reach`].
    pub fn identity(&self) -> &Did {
        &self.identity
    }

    /// The recipient's pubkey — whose key the secret is sealed to (who can
    /// decrypt). `None` only if `identity` is a non-key DID. Sugar for
    /// `identity().pubkey()`.
    pub fn recipient(&self) -> Option<PublicKey> {
        self.identity.pubkey()
    }

    /// The root ratchet state sealed to [`Self::identity`].
    pub fn ratchet_share(&self) -> &RatchetShare {
        &self.ratchet_share
    }

    /// The always-on host this client is reached through, if any. `None`
    /// means the client is dialed directly. This is the raw relay
    /// identity; for "where do I actually reach this share" use
    /// [`Self::reach`], which folds in the direct-dial fallback.
    pub fn via(&self) -> Option<&Did> {
        self.via.as_ref()
    }

    /// **Where to reach this share** — the dial/fetch target every
    /// transport path wants (announce a head, download a blob): the `via`
    /// host for a hosted recipient (e.g. the hub, which mirrors the
    /// blobs), else the recipient itself for a directly-dialable peer.
    ///
    /// This is the counterpart to [`Self::recipient`] ("who"): a browser
    /// share is `recipient = browser_key`, `reach = hub`. Callers that
    /// dial or fetch must use `reach`, not `recipient` — a browser has no
    /// iroh endpoint, so dialing the recipient directly fails.
    pub fn reach(&self) -> Option<PublicKey> {
        self.via
            .as_ref()
            .and_then(Did::pubkey)
            .or_else(|| self.identity.pubkey())
    }

    /// The root ratchet state as of `height`, for the holder of `key`:
    /// unseal the stored state and bring it forward by
    /// `height − sealed_at`.
    pub fn root_ratchet_at(
        &self,
        key: &PrivateKey,
        height: u64,
    ) -> Result<EntryRatchet, ShareError> {
        let sealed_at = self.ratchet_share.sealed_at();
        if sealed_at > height {
            return Err(ShareError::SealedAfterManifest { sealed_at, height });
        }
        let state = self.ratchet_share.recover(key)?;
        Ok(state.advanced_by(height - sealed_at))
    }
}

/// The set of peers who can decrypt the vault: a map from each peer's
/// [`PublicKey`] to the [`Share`] that grants them access.
///
/// Stored on the [`Manifest`](super::Manifest).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Shares(BTreeMap<PublicKey, Share>);

impl Shares {
    /// An empty shares map.
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Insert (or overwrite) the share for `key`.
    pub fn insert(&mut self, key: PublicKey, share: Share) {
        self.0.insert(key, share);
    }

    /// Look up `key`'s share.
    pub fn get(&self, key: &PublicKey) -> Option<&Share> {
        self.0.get(key)
    }

    /// True if `key` has a share recorded.
    pub fn contains_key(&self, key: &PublicKey) -> bool {
        self.0.contains_key(key)
    }

    /// Iterate the public keys.
    pub fn keys(&self) -> impl Iterator<Item = &PublicKey> {
        self.0.keys()
    }

    /// Iterate `(public_key, share)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&PublicKey, &Share)> {
        self.0.iter()
    }

    /// Number of shares.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True if there are no shares.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Remove and return `key`'s share, if present.
    pub fn remove(&mut self, key: &PublicKey) -> Option<Share> {
        self.0.remove(key)
    }

    /// The root ratchet state as of `height` for the holder of `key`.
    /// [`ShareError::NotFound`] when `key` has no share.
    pub fn root_ratchet_for(
        &self,
        key: &PrivateKey,
        height: u64,
    ) -> Result<EntryRatchet, ShareError> {
        self.get(&key.public())
            .ok_or(ShareError::NotFound)?
            .root_ratchet_at(key, height)
    }

    /// Re-seal every share to `root` at `sealed_at`. Called after the
    /// root lineage is re-seeded on revocation.
    pub fn reseal_all(&mut self, root: &EntryRatchet, sealed_at: u64) -> Result<(), ShareError> {
        for (pubkey, share) in self.0.iter_mut() {
            share.reseal(root, pubkey, sealed_at)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_share_sealed_at_a_height_derives_every_later_root_key() {
        // Alice seals the root state at height 3. Reading at height 5 must
        // produce the same key as stepping the live ratchet twice.
        let alice = PrivateKey::generate();
        let root = EntryRatchet::seed();
        let share = Share::seal(&root, &alice.public(), 3, None).unwrap();

        let at_five = share.root_ratchet_at(&alice, 5).unwrap();
        assert_eq!(at_five.key(), root.advanced_by(2).key());
        assert_eq!(share.root_ratchet_at(&alice, 3).unwrap().key(), root.key());
    }

    #[test]
    fn a_share_newer_than_its_manifest_is_rejected() {
        let alice = PrivateKey::generate();
        let share = Share::seal(&EntryRatchet::seed(), &alice.public(), 4, None).unwrap();

        assert!(matches!(
            share.root_ratchet_at(&alice, 3),
            Err(ShareError::SealedAfterManifest {
                sealed_at: 4,
                height: 3
            })
        ));
    }

    #[test]
    fn only_members_recover_the_root() {
        let alice = PrivateKey::generate();
        let bob = PrivateKey::generate();
        let root = EntryRatchet::seed();
        let mut shares = Shares::new();
        shares.insert(
            alice.public(),
            Share::seal(&root, &alice.public(), 0, None).unwrap(),
        );

        assert!(shares.root_ratchet_for(&alice, 0).is_ok());
        assert!(matches!(
            shares.root_ratchet_for(&bob, 0),
            Err(ShareError::NotFound)
        ));
    }

    #[test]
    fn resealing_keeps_identity_and_via_but_swaps_the_lineage() {
        let alice = PrivateKey::generate();
        let hub = PrivateKey::generate().public();
        let old = EntryRatchet::seed();
        let new = EntryRatchet::seed();
        let mut shares = Shares::new();
        shares.insert(
            alice.public(),
            Share::seal(&old, &alice.public(), 2, Some(Did::from_key(&hub))).unwrap(),
        );

        shares.reseal_all(&new, 7).unwrap();
        let share = shares.get(&alice.public()).unwrap();
        assert_eq!(share.via(), Some(&Did::from_key(&hub)));
        assert_eq!(share.ratchet_share().sealed_at(), 7);
        assert_eq!(share.root_ratchet_at(&alice, 7).unwrap().key(), new.key());
    }

    #[test]
    fn share_roundtrips_through_dag_cbor() {
        use ipld_core::codec::Codec;
        use serde_ipld_dagcbor::codec::DagCborCodec;

        let alice = PrivateKey::generate().public();
        let share = Share::seal(&EntryRatchet::seed(), &alice, 0, None).unwrap();

        let encoded = DagCborCodec::encode_to_vec(&share).unwrap();
        let decoded: Share = DagCborCodec::decode_from_slice(&encoded).unwrap();

        assert_eq!(share, decoded);
    }
}
