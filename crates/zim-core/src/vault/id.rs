//! Self-certifying vault identity.
//!
//! A [`VaultId`] is the blake3 hash of the vault's genesis manifest
//! blob — the same hash the genesis [`Link`](crate::linked_data::Link)
//! carries. Identity is *derived*, never declared: manifests don't
//! embed an id field, so there is nothing to forge. Whether a chain
//! belongs to a vault is decided by walking `previous` links to
//! genesis and hashing it.
//!
//! Two consequences the sync layer leans on:
//!
//! 1. **No collisions by construction.** Genesis carries a random
//!    nonce, so two vaults can't share an id without a blake3
//!    collision — and a peer can't mint a second vault under a known
//!    id at all.
//! 2. **Common ancestors always exist.** Two verified chains for the
//!    same id terminate at the same genesis, so "same id, unrelated
//!    history" is unrepresentable. The divergence case is rejected at
//!    chain-verification time, not handled downstream.

use std::fmt::{self, Debug, Display};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::linked_data::{Hash, Link};

/// The blake3 hash of a vault's genesis manifest blob.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, std::hash::Hash)]
pub struct VaultId(Hash);

impl VaultId {
    /// Wrap a raw hash. The caller asserts this is a genesis-blob
    /// hash — use [`Self::from_genesis_link`] where a [`Link`] is at
    /// hand.
    pub fn from_hash(hash: Hash) -> Self {
        Self(hash)
    }

    /// Identity of the vault whose genesis manifest `link` addresses.
    pub fn from_genesis_link(link: &Link) -> Self {
        Self(link.hash())
    }

    pub fn hash(&self) -> Hash {
        self.0
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }
}

/// All-zero placeholder. Exists for `#[derive(Default)]` request
/// structs in the HTTP client — never a real identity (a real id is
/// always the hash of an actual genesis blob).
impl Default for VaultId {
    fn default() -> Self {
        Self(Hash::from_bytes([0; 32]))
    }
}

impl Display for VaultId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Lower hex, same rendering as blob hashes.
        write!(f, "{}", self.0)
    }
}

impl Debug for VaultId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VaultId({})", self.0)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid vault id: {0}")]
pub struct VaultIdParseError(String);

impl FromStr for VaultId {
    type Err = VaultIdParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Strict 64-char lower/upper hex. Deliberately NOT delegating
        // to `Hash::from_str` — iroh's impl asserts (panics!) on
        // inputs whose base32 decode isn't exactly 32 bytes, and this
        // parser runs on user input ("is this arg an id or a name?").
        let s = s.trim();
        let mut bytes = [0u8; 32];
        hex::decode_to_slice(s, &mut bytes)
            .map_err(|_| VaultIdParseError(format!("expected 64 hex chars, got {:?}", s)))?;
        Ok(Self(Hash::from_bytes(bytes)))
    }
}

// Custom serde: hex string in human-readable formats (JSON, URLs) so
// ids stay copy-pasteable; raw 32 bytes in binary formats (wire,
// CBOR) so they stay compact.
impl Serialize for VaultId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serializer.serialize_str(&self.to_string())
        } else {
            serializer.serialize_bytes(self.0.as_bytes())
        }
    }
}

impl<'de> Deserialize<'de> for VaultId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            s.parse().map_err(serde::de::Error::custom)
        } else {
            struct BytesVisitor;
            impl serde::de::Visitor<'_> for BytesVisitor {
                type Value = [u8; 32];
                fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.write_str("32 bytes")
                }
                fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                    v.try_into()
                        .map_err(|_| E::custom("vault id must be 32 bytes"))
                }
            }
            let arr = deserializer.deserialize_bytes(BytesVisitor)?;
            Ok(Self(Hash::from_bytes(arr)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let id = VaultId::from_hash(Hash::new(b"genesis"));
        let parsed: VaultId = id.to_string().parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn json_is_hex_string() {
        let id = VaultId::from_hash(Hash::new(b"genesis"));
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
        let back: VaultId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }
}
