pub use ipld_core::cid::multihash::Multihash;
pub use ipld_core::cid::{multibase, Cid, Error as CidError};
pub use ipld_core::codec::Codec;
pub use ipld_core::ipld::Ipld as LinkedData;

// Raw ipld codec
pub const LD_RAW_CODEC: u64 = 0x55;
pub const LD_CBOR_CODEC: u64 = 0x71;

pub const BLAKE3_HASH_CODE: u64 = 0x1e;

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("encoding error")]
    EncodeError,
    #[error("decoding error")]
    DecodeError,
}

/// Encode/decode via DAG-CBOR. Automatically implemented for any type that is
/// `Serialize + DeserializeOwned` where `DagCborCodec: Codec<T>`.
pub trait BlockEncoded: serde::Serialize + serde::de::DeserializeOwned {
    fn encode(&self) -> Result<Vec<u8>, CodecError>;
    fn decode(data: &[u8]) -> Result<Self, CodecError>;
    fn codec(&self) -> u64;
}

/// Blanket: every serde-compatible type with a DagCborCodec impl gets this.
impl<T> BlockEncoded for T
where
    T: serde::Serialize + serde::de::DeserializeOwned,
    serde_ipld_dagcbor::codec::DagCborCodec: Codec<T>,
{
    fn encode(&self) -> Result<Vec<u8>, CodecError> {
        <serde_ipld_dagcbor::codec::DagCborCodec as Codec<T>>::encode_to_vec(self)
            .map_err(|_| CodecError::EncodeError)
    }
    fn decode(data: &[u8]) -> Result<Self, CodecError> {
        <serde_ipld_dagcbor::codec::DagCborCodec as Codec<T>>::decode_from_slice(data)
            .map_err(|_| CodecError::DecodeError)
    }
    fn codec(&self) -> u64 {
        LD_CBOR_CODEC
    }
}
