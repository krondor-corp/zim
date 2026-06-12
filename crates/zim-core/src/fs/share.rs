//! Vault access primitives stored on the [`Manifest`](super::Manifest):
//! [`Share`] (a peer's encrypted handle to vault content) and [`Relay`]
//! (a peer authorized to mirror the published-set without holding any
//! secret).
//!
//! Both types carry an [`Identity`] (`did:key` or `did:web`) rather
//! than a raw pubkey — see `zim-did` for the type. For `did:key`
//! identities the pubkey is encoded in the DID itself; for `did:web`
//! it lives in the resolved DID document. The vault save loop is
//! responsible for resolving `did:web` shares into per-verification-
//! method `did:key` entries at save time (the "expansion" described
//! in `docs/research/hub-revival.md`). Until that lands every Share
//! on disk is `Identity::Key`.

use serde::{Deserialize, Serialize};

use zim_crypto::SecretShare;
use zim_did::Identity;

/// A relay peer authorized to serve a vault's published-set.
///
/// Relays never hold the vault secret — they only need the public
/// metadata pack plus pinned file blobs to serve the
/// [`Published`](super::Published) set over the gateway.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Relay {
    identity: Identity,
}

impl Relay {
    /// Register `identity` as a relay.
    pub fn new(identity: Identity) -> Self {
        Self { identity }
    }

    /// The relay peer's logical identity (DID).
    pub fn identity(&self) -> &Identity {
        &self.identity
    }
}

/// A peer's share of vault access.
///
/// Pairs an [`Identity`] (DID) with a [`SecretShare`] — the vault
/// secret encrypted to that identity's pubkey. The peer recovers the
/// vault secret by decrypting `secret_share` with their private key.
///
/// The previous `dialable: bool` field is gone: dialability is now
/// derived from the DID method (and, eventually, the verification
/// method's declared purpose in the DID document). `Identity::Key`
/// is always dialable as an iroh peer; `Identity::Web` is dialable
/// only when the resolved DID doc lists a `peer`-purpose verification
/// method matching the share's underlying pubkey.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Share {
    identity: Identity,
    secret_share: SecretShare,
}

impl Share {
    /// Construct a share.
    ///
    /// - `secret_share` — the vault secret encrypted to `identity`'s
    ///   underlying pubkey.
    /// - `identity` — the peer's DID-shaped identity.
    pub fn new(secret_share: SecretShare, identity: Identity) -> Self {
        Self {
            identity,
            secret_share,
        }
    }

    /// The peer's logical identity (DID).
    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    /// The vault secret encrypted to [`Self::identity`].
    pub fn secret_share(&self) -> &SecretShare {
        &self.secret_share
    }

    /// Replace the encrypted secret share (called at save time when the
    /// vault secret rotates and shares are re-minted).
    pub fn set_secret_share(&mut self, secret_share: SecretShare) {
        self.secret_share = secret_share;
    }
}
