use serde::{Deserialize, Serialize};

use super::hash::Hash;
use super::ipld::{Cid, LinkedData, Multihash, BLAKE3_HASH_CODE, LD_CBOR_CODEC, LD_RAW_CODEC};

/// A content-addressed link: a CID wrapping a blake3 hash with a codec tag.
/// On native builds only, can be converted to iroh-blobs transport types.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Link(Cid);

impl std::fmt::Display for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.hash())
    }
}

impl Default for Link {
    fn default() -> Self {
        let hash = Hash::from_bytes([0; 32]);
        let mh = Multihash::wrap(BLAKE3_HASH_CODE, hash.as_bytes()).expect("valid blake3 hash");
        Link(Cid::new_v1(LD_RAW_CODEC, mh))
    }
}

impl From<Link> for Cid {
    fn from(val: Link) -> Self {
        val.0
    }
}

impl From<Cid> for Link {
    fn from(val: Cid) -> Self {
        let hash = val.hash();
        let code = hash.code();
        let codec = val.codec();
        if code != BLAKE3_HASH_CODE {
            panic!("invalid hash code");
        }
        if codec != LD_RAW_CODEC && codec != LD_CBOR_CODEC {
            panic!("unsupported codec");
        }
        Link(val)
    }
}

impl From<Link> for LinkedData {
    fn from(val: Link) -> Self {
        LinkedData::Link(val.0)
    }
}

impl From<Link> for Hash {
    fn from(val: Link) -> Self {
        val.hash()
    }
}

impl From<&Link> for Hash {
    fn from(val: &Link) -> Self {
        val.hash()
    }
}

// Native-only: conversion to iroh_blobs transport types.
#[cfg(feature = "native")]
impl From<Link> for iroh_blobs::HashAndFormat {
    fn from(val: Link) -> Self {
        iroh_blobs::HashAndFormat {
            hash: iroh_blobs::Hash::from(val.hash()),
            format: iroh_blobs::BlobFormat::Raw,
        }
    }
}

#[cfg(feature = "native")]
impl From<Link> for iroh_blobs::Hash {
    fn from(val: Link) -> Self {
        iroh_blobs::Hash::from(val.hash())
    }
}

#[cfg(feature = "native")]
impl From<&Link> for iroh_blobs::Hash {
    fn from(val: &Link) -> Self {
        iroh_blobs::Hash::from(val.hash())
    }
}

impl Link {
    pub fn new(codec: u64, hash: Hash) -> Self {
        let mh = Multihash::wrap(BLAKE3_HASH_CODE, hash.as_bytes()).expect("valid blake3 hash");
        Link(Cid::new_v1(codec, mh))
    }

    pub fn codec(&self) -> u64 {
        self.0.codec()
    }

    pub fn hash(&self) -> Hash {
        let digest = self.0.hash().digest();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(digest);
        Hash::from_bytes(bytes)
    }

    pub fn cid(&self) -> &Cid {
        &self.0
    }

    /// Iroh-blobs ticket for peer-to-peer transfer. Native only.
    #[cfg(feature = "native")]
    pub fn ticket(
        &self,
        source: zim_crypto::PublicKey,
        format: Option<iroh_blobs::BlobFormat>,
    ) -> iroh_blobs::ticket::BlobTicket {
        let node_addr = iroh::NodeAddr::new(crate::iroh::to_iroh_public_key(&source));
        iroh_blobs::ticket::BlobTicket::new(
            node_addr,
            iroh_blobs::Hash::from(self.hash()),
            format.unwrap_or(iroh_blobs::BlobFormat::Raw),
        )
    }
}
