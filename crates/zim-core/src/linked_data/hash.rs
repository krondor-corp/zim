use std::fmt;
use std::str::FromStr;

use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Content hash — 32 raw blake3 bytes.
///
/// Wire-compatible with `iroh_blobs::Hash`: binary formats get raw bytes,
/// human-readable formats (JSON) get a lowercase hex string.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, std::hash::Hash)]
pub struct Hash([u8; 32]);

impl Hash {
    /// Compute the blake3 hash of `data`.
    pub fn new(data: &[u8]) -> Self {
        Self(*blake3::hash(data).as_bytes())
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({})", self.to_hex())
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl From<[u8; 32]> for Hash {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl From<Hash> for [u8; 32] {
    fn from(h: Hash) -> Self {
        h.0
    }
}

#[derive(Debug)]
pub struct HashParseError;

impl fmt::Display for HashParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid hex hash: expected 64 hex chars")
    }
}

impl std::error::Error for HashParseError {}

impl FromStr for Hash {
    type Err = HashParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = hex::decode(s).map_err(|_| HashParseError)?;
        let arr: [u8; 32] = bytes.try_into().map_err(|_| HashParseError)?;
        Ok(Hash(arr))
    }
}

impl Serialize for Hash {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if s.is_human_readable() {
            s.serialize_str(&self.to_hex())
        } else {
            s.serialize_bytes(&self.0)
        }
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        if d.is_human_readable() {
            let s = String::deserialize(d)?;
            s.parse::<Hash>().map_err(serde::de::Error::custom)
        } else {
            struct V;
            impl<'de> Visitor<'de> for V {
                type Value = Hash;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    write!(f, "32 bytes (blake3 hash)")
                }
                fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Hash, E> {
                    let arr: [u8; 32] = v.try_into().map_err(|_| E::custom("expected 32 bytes"))?;
                    Ok(Hash(arr))
                }
            }
            d.deserialize_bytes(V)
        }
    }
}
