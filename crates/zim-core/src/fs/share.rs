//! [`Share`] — a peer's encrypted handle to vault content, stored on
//! the [`Manifest`](super::Manifest).
//!
//! A share carries an [`Identity`] (`did:key` or `did:web`) rather than
//! a raw pubkey — see `zim-did`. It also carries an optional `via`: the
//! always-on host a hosted client is reached *through*. This is what
//! folds the old separate `Relay` type away — a relay is just a share
//! with a `via` set (see the hosted-DID protocol in
//! `docs/concepts/identity.md`):
//!
//! - **`via = None`** — direct: the client is dialed as an iroh peer
//!   (its `NodeId` is the key). `did:key` shares.
//! - **`via = Some(host)`** — hosted: the secret is still sealed to the
//!   client (zero-knowledge — the host never holds it), but sync dials
//!   the host, never the client. The host is recorded as a resolved
//!   `Identity::Key`, so routing/access checks stay synchronous.
//!
//! The caller (which owns a DID resolver) expands a `did:web` into one
//! share per verification method at share time via
//! [`zim_did::resolve_reaches`], sealing each client and stamping the
//! shared `via`. Every `Share` persisted on disk therefore carries a
//! concrete `Identity::Key` for both `identity` and `via`.

use serde::{Deserialize, Serialize};

use zim_crypto::{PublicKey, SecretShare};
use zim_did::Identity;

/// A peer's share of vault access.
///
/// Pairs an [`Identity`] (the seal target) with a [`SecretShare`] (the
/// vault secret encrypted to that identity's pubkey) and an optional
/// `via` host the client is reached through.
///
/// Dialability is derived, never stored as a flag: a share with
/// `via = None` is dialed directly; one with `via = Some(host)` is
/// reached through `host`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Share {
    identity: Identity,
    secret_share: SecretShare,
    /// The always-on host this client is reached through. `None` for a
    /// directly-dialable peer; `Some(Identity::Key(host))` for a hosted
    /// client (e.g. a browser reached via the hub). The host never holds
    /// the vault secret — `secret_share` is sealed to the client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    via: Option<Identity>,
}

impl Share {
    /// Construct a share.
    ///
    /// - `secret_share` — the vault secret encrypted to `identity`'s
    ///   underlying pubkey.
    /// - `identity` — the peer's DID-shaped identity (the seal target).
    /// - `via` — the host the client is reached through, or `None` for a
    ///   directly-dialable peer.
    pub fn new(secret_share: SecretShare, identity: Identity, via: Option<Identity>) -> Self {
        Self {
            identity,
            secret_share,
            via,
        }
    }

    // Two distinct questions a share answers — keep them apart:
    //   * `identity` / `recipient` — *who* gets the secret (who decrypts).
    //   * `via` / `reach`          — *where* we dial or fetch for them.
    // They coincide for a directly-dialable peer and diverge for a hosted
    // one (a browser whose data lives on the hub).

    /// The peer's logical identity (DID) — the seal target. This is *who*
    /// the share is for, not where to reach them; see [`Self::reach`].
    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    /// The recipient's pubkey — whose key the secret is sealed to (who can
    /// decrypt). `None` only if `identity` is a non-key DID. Sugar for
    /// `identity().pubkey()`.
    pub fn recipient(&self) -> Option<&PublicKey> {
        self.identity.pubkey()
    }

    /// The vault secret encrypted to [`Self::identity`].
    pub fn secret_share(&self) -> &SecretShare {
        &self.secret_share
    }

    /// The always-on host this client is reached through, if any. `None`
    /// means the client is dialed directly. This is the raw relay
    /// identity; for "where do I actually reach this share" use
    /// [`Self::reach`], which folds in the direct-dial fallback.
    pub fn via(&self) -> Option<&Identity> {
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
    pub fn reach(&self) -> Option<&PublicKey> {
        self.via
            .as_ref()
            .and_then(Identity::pubkey)
            .or_else(|| self.identity.pubkey())
    }

    /// Replace the encrypted secret share (called at save time when the
    /// vault secret rotates and shares are re-minted).
    pub fn set_secret_share(&mut self, secret_share: SecretShare) {
        self.secret_share = secret_share;
    }
}
