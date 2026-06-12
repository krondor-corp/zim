//! [`Identity`] — the type that replaces raw `PublicKey` in every
//! "this is a peer" position across the workspace.
//!
//! Two variants today:
//!
//! - [`Identity::Key`] — wraps an ed25519 [`PublicKey`]. Equivalent
//!   to `did:key:<encoded>`; we can move between the two losslessly
//!   without resolution.
//! - [`Identity::Web`] — wraps a parsed `did:web:` URL plus a cached
//!   document fingerprint. The fingerprint is `None` until the
//!   document is first resolved; subsequent resolutions compare and
//!   surface rotation. Resolution itself is not in this crate.
//!
//! ## When code wants a pubkey
//!
//! Lots of downstream code needs a concrete ed25519 pubkey: the iroh
//! transport dials it, `SecretShare` encrypts to it, the spam gate
//! compares against it. [`Identity::pubkey`] returns one *if* the
//! identity carries a single concrete key:
//!
//! - `Key(pk)` → `Some(pk)`
//! - `Web { … }` → `None` (resolution is needed; out of scope here)
//!
//! The save-time DID expansion described in the plan is the bridge:
//! a `Share(Identity::Web(did))` entry is resolved at save time into
//! N per-verification-method `Share(Identity::Key(vm_pubkey))`
//! entries. By the time a Share is on disk, it's always `Key`.

use serde::{Deserialize, Serialize};
use zim_crypto::PublicKey;

use crate::did::Did;
use crate::did_key::{did_key_decode, did_key_encode};

/// Logical identity of a peer (daemon, hub, or browser/device).
///
/// Serializes as externally-tagged JSON / DAG-CBOR — the variant
/// name is the map key:
///
/// ```json
/// { "Key": <pubkey-bytes> }
/// { "Web": { "did": "did:web:...", "cached_fingerprint": null } }
/// ```
///
/// Internally-tagged (`#[serde(tag = "method")]`) is tempting but
/// breaks for the `Key` newtype: DAG-CBOR requires a map payload to
/// merge the tag in, and `PublicKey` serializes as bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Identity {
    /// Self-describing — the DID encodes the ed25519 pubkey directly.
    /// Maps to `did:key:<multibase>`. Used by daemons + (for now)
    /// hubs at the protocol layer.
    Key(PublicKey),

    /// Resolved via HTTPS at use time. `cached_fingerprint` lets
    /// observers detect a hub silently rotating the document under
    /// them — if it changes from what we last saw, anything that
    /// relied on the old document needs re-validating.
    Web {
        /// The full `did:web:` URL, optionally with a verification-
        /// method fragment (`did:web:hub.example.com:u:alice#laptop`).
        /// Stored as a `Did` so parse failures surface at
        /// construction, not at use site.
        did: Did,
        /// `None` until the doc is first resolved.
        cached_fingerprint: Option<String>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error(transparent)]
    Did(#[from] crate::did::DidError),
    #[error("unsupported DID method `{0}` — only did:key and did:web are recognized")]
    UnsupportedMethod(String),
    #[error("did:key decode: {0}")]
    DidKey(String),
}

impl Identity {
    /// The concrete ed25519 pubkey when one is available *without*
    /// resolution. `None` for `Web` identities — callers needing a
    /// pubkey for those must drive a resolver themselves.
    pub fn pubkey(&self) -> Option<&PublicKey> {
        match self {
            Identity::Key(pk) => Some(pk),
            Identity::Web { .. } => None,
        }
    }

    /// Render this identity as a canonical DID URL.
    pub fn to_did(&self) -> Did {
        match self {
            Identity::Key(pk) => Did::parse(&format!("did:key:{}", did_key_encode(pk)))
                .expect("did:key encode produces a valid DID"),
            Identity::Web { did, .. } => did.clone(),
        }
    }

    /// Build an [`Identity`] from an already-parsed [`Did`]. For
    /// `did:key`, decodes the embedded pubkey. For `did:web`, stores
    /// the DID with no cached fingerprint (set it later when the doc
    /// is first resolved).
    pub fn from_did(did: Did) -> Result<Self, IdentityError> {
        match did.method() {
            crate::did::DidMethod::Key => {
                let pk = did_key_decode(did.identifier()).map_err(IdentityError::DidKey)?;
                Ok(Identity::Key(pk))
            }
            crate::did::DidMethod::Web => Ok(Identity::Web {
                did,
                cached_fingerprint: None,
            }),
            crate::did::DidMethod::Other(m) => Err(IdentityError::UnsupportedMethod(m.into())),
        }
    }

    /// Parse a DID URL string into an [`Identity`]. Convenience: this
    /// is what every CLI / HTTP arg parser wants.
    pub fn parse(s: &str) -> Result<Self, IdentityError> {
        Self::from_did(Did::parse(s)?)
    }
}

impl std::fmt::Display for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_did())
    }
}

impl From<PublicKey> for Identity {
    fn from(pk: PublicKey) -> Self {
        Identity::Key(pk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zim_crypto::PrivateKey;

    #[test]
    fn key_identity_roundtrip_via_did_string() {
        let pk = PrivateKey::generate().public();
        let id = Identity::Key(pk);
        let did_string = id.to_string();
        assert!(did_string.starts_with("did:key:z"));
        let back = Identity::parse(&did_string).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn web_identity_parse_holds_did_and_no_fingerprint() {
        let id = Identity::parse("did:web:hub.example.com:u:alice").unwrap();
        match id {
            Identity::Web {
                did,
                cached_fingerprint,
            } => {
                assert_eq!(did.as_str(), "did:web:hub.example.com:u:alice");
                assert!(cached_fingerprint.is_none());
            }
            _ => panic!("expected Web variant"),
        }
    }

    #[test]
    fn key_identity_yields_pubkey_directly() {
        let pk = PrivateKey::generate().public();
        let id = Identity::Key(pk);
        assert_eq!(id.pubkey().copied(), Some(pk));
    }

    #[test]
    fn web_identity_pubkey_requires_resolution() {
        let id = Identity::parse("did:web:hub.example.com").unwrap();
        assert!(id.pubkey().is_none());
    }
}
