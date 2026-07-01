//! The [`Manifest`]: a vault's signed top-level record.
//!
//! Each save produces a new manifest pointing back at the previous one,
//! forming an immutable history chain. The manifest carries everything a
//! peer needs to access the vault's current state:
//!
//! - **Identity** — friendly name only. The vault's id is *derived*:
//!   `blake3(genesis manifest blob)`. Manifests never embed an id —
//!   a declared id would be forgeable, while ancestry is not (see
//!   `zim_core::vault::VaultId`). Genesis carries a random `nonce`
//!   so identical-content vaults can't collide on the derived id.
//! - **Access control** — [`Shares`] map (peer → encrypted vault secret).
//!   A hosted client's share carries a `via` host, folding the old
//!   separate relay list into the share itself.
//! - **Content pointers** — [`Link`] to the root dir body, [`Pins`] of
//!   blobs that live outside this manifest, inline
//!   [`Metadata`](super::content_store::Metadata) pack of dir bodies,
//!   [`Published`] map.
//! - **History** — `previous` link to the prior version, `height` in the
//!   chain, encrypted ops-log link, and the Lamport clock at save time.
//! - **Author + signature** — Ed25519 over [`Manifest::signable_bytes`].
//!
//! # Versioning
//!
//! Each save increments `height` by 1 and sets `previous` to the prior
//! manifest's [`Link`]. Genesis manifests have `height = 0` and a
//! default `previous`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::linked_data::{BlockEncoded, CodecError, Link};
use zim_crypto::{PrivateKey, PublicKey, SecretShare, Signature};
use zim_did::Identity;

use super::abs_path::AbsPath;
use super::content_store::Metadata;
use super::entry::Entry;
use super::pins::Pins;
use super::published::Published;
use super::share::Share;

/// Version type for manifest bookkeeping (kept as a string alias).
pub type Version = String;

/// The set of peers who can decrypt the vault: a map from each peer's
/// [`PublicKey`] to the [`Share`] that grants them access.
///
/// Stored on the [`Manifest`]; mutated by [`Fs::add_share`](super::Fs::add_share).
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

    /// Mutable `(public_key, share)` iterator. The save loop uses
    /// this to re-mint each share's encrypted secret against the
    /// stored pubkey — no need to fish a pubkey back out of the
    /// share's `Identity`.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&PublicKey, &mut Share)> {
        self.0.iter_mut()
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
}

/// Errors raised by manifest [`encode`](BlockEncoded::encode) /
/// [`decode`](BlockEncoded::decode) / [`sign`](Manifest::sign) /
/// [`verify_signature`](Manifest::verify_signature).
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    /// DAG-CBOR encode/decode failed.
    #[error("codec error: {0}")]
    Codec(#[from] CodecError),
    /// `verify` rejected the signature.
    #[error("signature verification failed")]
    SignatureVerificationFailed,
    /// The manifest claims an author that isn't in `shares` — a peer
    /// can't sign a manifest they don't have access to.
    #[error("author not in shares")]
    UnauthorizedAuthor,
}

/// The root metadata structure for a vault.
///
/// Serialized using DAG-CBOR; the CID serves as the vault's state identifier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Random salt minted at genesis and carried forward verbatim by
    /// every save. Guarantees two vaults with identical initial
    /// content still hash to distinct genesis blobs — i.e. distinct
    /// `VaultId`s. No other semantics; never read back.
    nonce: [u8; 16],
    /// Human-readable name for display.
    name: String,
    /// Height in the version chain (0 for initial, increments on each update).
    height: u64,
    /// Software version for compatibility checking.
    version: Version,
    /// Map of peer public keys (hex) to their shares. A hosted client's
    /// share carries a `via` host — relays are folded in here, not a
    /// separate list.
    shares: Shares,
    /// Link to the root [`Dir`](Dir) of the file tree.
    root: Link,
    /// Pinned content hashes (inline).
    #[serde(default)]
    pins: Pins,
    /// Link to the previous manifest version. Default for genesis.
    previous: Link,
    /// Link to the encrypted path operations log (CRDT). Default if empty.
    ops: Link,
    /// Lamport clock value at the time of this save. On load, the in-memory
    /// `OpsLog` is seeded with this so newly-recorded ops stay monotonic.
    #[serde(default)]
    ops_clock: u64,
    /// All dir bodies packed inline. Loaded into the ContentStore on mount.
    #[serde(default)]
    metadata: Metadata,
    /// Paths publicly served by the gateway.
    #[serde(default, skip_serializing_if = "Published::is_empty")]
    published: Published,
    author: PublicKey,
    /// Ed25519 signature over the manifest contents.
    /// Covers all fields except `signature` itself (see [`Manifest::signable_bytes`]).
    signature: Signature,
}

impl Manifest {
    pub fn new(
        name: String,
        secret_key: &PrivateKey,
        share: SecretShare,
        root: Link,
        height: u64,
    ) -> Result<Self, ManifestError> {
        let owner = secret_key.public();
        let mut nonce = [0u8; 16];
        getrandom::getrandom(&mut nonce).expect("os rng");
        let mut manifest = Manifest {
            nonce,
            name,
            shares: {
                let mut s = Shares::new();
                s.insert(owner, Share::new(share, Identity::Key(owner), None));
                s
            },
            root,
            pins: Pins::default(),
            previous: Link::default(),
            height,
            version: Version::default(),
            ops: Link::default(),
            ops_clock: 0,
            metadata: Metadata::new(),
            published: BTreeMap::new(),
            author: secret_key.public(),
            signature: Signature::from_bytes(&[0u8; 64]),
        };
        manifest.sign(secret_key)?;
        Ok(manifest)
    }

    /* Getters */

    /// Get the vault's display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the software version.
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Get the root node link.
    pub fn root(&self) -> &Link {
        &self.root
    }

    pub fn pins(&self) -> &Pins {
        &self.pins
    }

    pub fn pins_mut(&mut self) -> &mut Pins {
        &mut self.pins
    }

    pub fn previous(&self) -> &Link {
        &self.previous
    }

    /// Get the version chain height.
    pub fn height(&self) -> u64 {
        self.height
    }

    pub fn ops(&self) -> &Link {
        &self.ops
    }

    pub fn ops_clock(&self) -> u64 {
        self.ops_clock
    }

    pub fn set_ops_clock(&mut self, clock: u64) {
        self.ops_clock = clock;
    }

    pub fn shares(&self) -> &Shares {
        &self.shares
    }

    pub fn shares_mut(&mut self) -> &mut Shares {
        &mut self.shares
    }

    pub fn get_share(&self, public_key: &PublicKey) -> Option<&Share> {
        self.shares.get(public_key)
    }

    pub fn get_peer_ids(&self) -> Vec<PublicKey> {
        self.shares.keys().cloned().collect()
    }

    pub fn author(&self) -> &PublicKey {
        &self.author
    }

    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    /* Setters */

    /// Set the root node link.
    pub fn set_root(&mut self, root: Link) {
        self.root = root;
    }

    pub fn set_pins(&mut self, pins: Pins) {
        self.pins = pins;
    }

    pub fn set_previous(&mut self, previous: Link) {
        self.previous = previous;
    }

    /// Set the version chain height.
    pub fn set_height(&mut self, height: u64) {
        self.height = height;
    }

    pub fn set_ops(&mut self, link: Link) {
        self.ops = link;
    }

    pub fn reset_ops(&mut self) {
        self.ops = Link::default();
    }

    /// Insert a share keyed by `pubkey`. The caller resolves the
    /// share recipient's identity to a concrete pubkey (the share's
    /// SecretShare is encrypted to it); we don't reach back into
    /// `share.identity()` for that — `Identity::Web` shares wouldn't
    /// carry one and this method has no business doing DID
    /// resolution.
    pub fn add_share(&mut self, pubkey: PublicKey, share: Share) {
        self.shares.insert(pubkey, share);
    }

    pub fn has_share(&self, public_key: &PublicKey) -> bool {
        self.shares.contains_key(public_key)
    }

    /// True if `public_key` is the `via` host of any share — i.e. this
    /// peer relays/mirrors this vault on a hosted client's behalf. The
    /// hub uses this to decide whether it should mirror a vault.
    pub fn is_via(&self, public_key: &PublicKey) -> bool {
        self.shares
            .iter()
            .any(|(_, s)| s.via().and_then(|v| v.pubkey()) == Some(public_key))
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    pub fn set_metadata(&mut self, metadata: Metadata) {
        self.metadata = metadata;
    }

    pub fn published(&self) -> &Published {
        &self.published
    }

    pub fn published_mut(&mut self) -> &mut Published {
        &mut self.published
    }

    pub fn publish(&mut self, path: AbsPath, leaf: Entry) {
        self.published.insert(path, leaf);
    }

    pub fn unpublish(&mut self, path: &AbsPath) -> bool {
        self.published.remove(path).is_some()
    }

    /* Signing */

    pub fn sign(&mut self, secret_key: &PrivateKey) -> Result<(), ManifestError> {
        self.author = secret_key.public();
        let bytes = self.signable_bytes()?;
        self.signature = secret_key.sign(&bytes);
        Ok(())
    }

    /// Verify the author was authorized to create this manifest.
    ///
    /// Checks signature validity and that the author exists in the
    /// shares of `previous` (or self if genesis).
    pub fn verify_author(&self, previous: Option<&Manifest>) -> Result<(), ManifestError> {
        self.verify_signature()?;
        let check_shares = previous.map(|p| &p.shares).unwrap_or(&self.shares);
        if !check_shares.contains_key(&self.author) {
            return Err(ManifestError::UnauthorizedAuthor);
        }
        Ok(())
    }

    pub fn verify_signature(&self) -> Result<(), ManifestError> {
        let bytes = self.signable_bytes()?;
        self.author
            .verify(&bytes, &self.signature)
            .map_err(|_| ManifestError::SignatureVerificationFailed)?;
        Ok(())
    }

    fn signable_bytes(&self) -> Result<Vec<u8>, ManifestError> {
        let mut signable = self.clone();
        signable.signature = Signature::from_bytes(&[0u8; 64]);
        Ok(signable.encode()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linked_data::Link;
    #[allow(unused_imports)]
    use zim_crypto::{PublicKey, Secret};

    fn create_test_manifest(secret_key: &PrivateKey) -> Manifest {
        Manifest::new(
            "test-vault".to_string(),
            secret_key,
            SecretShare::default(),
            Link::default(),
            0,
        )
        .unwrap()
    }

    #[test]
    fn test_share_struct_serialize() {
        use ipld_core::codec::Codec;
        use serde_ipld_dagcbor::codec::DagCborCodec;

        let public_key = zim_crypto::PrivateKey::generate().public();
        let share = Share::new(SecretShare::default(), Identity::Key(public_key), None);

        let encoded = DagCborCodec::encode_to_vec(&share).unwrap();
        let decoded: Share = DagCborCodec::decode_from_slice(&encoded).unwrap();

        assert_eq!(share, decoded);
    }

    #[test]
    fn test_manifest_signed_on_creation() {
        let secret_key = PrivateKey::generate();
        let manifest = create_test_manifest(&secret_key);

        assert_eq!(manifest.author(), &secret_key.public());
        manifest.verify_signature().unwrap();
    }

    #[test]
    fn test_manifest_tamper_detection() {
        let secret_key = PrivateKey::generate();
        let mut manifest = create_test_manifest(&secret_key);

        manifest.verify_signature().unwrap();
        manifest.set_height(999);

        assert!(manifest.verify_signature().is_err());
    }

    #[test]
    fn test_manifest_roundtrip() {
        use ipld_core::codec::Codec;
        use serde_ipld_dagcbor::codec::DagCborCodec;

        let secret_key = PrivateKey::generate();
        let manifest = create_test_manifest(&secret_key);

        let encoded = DagCborCodec::encode_to_vec(&manifest).unwrap();
        let decoded: Manifest = DagCborCodec::decode_from_slice(&encoded).unwrap();

        assert_eq!(decoded.author(), &secret_key.public());
        decoded.verify_signature().unwrap();
    }

    #[test]
    fn test_manifest_resign() {
        let secret_key1 = PrivateKey::generate();
        let secret_key2 = PrivateKey::generate();
        let mut manifest = create_test_manifest(&secret_key1);

        assert_eq!(manifest.author(), &secret_key1.public());

        manifest.sign(&secret_key2).unwrap();
        assert_eq!(manifest.author(), &secret_key2.public());
        manifest.verify_signature().unwrap();
    }
}
