// Key conversions
use zim_crypto::{PrivateKey, PublicKey};

pub fn to_iroh_public_key(key: &PublicKey) -> iroh::PublicKey {
    iroh::PublicKey::from_bytes(&key.to_bytes()).expect("valid public key")
}

pub fn from_iroh_public_key(key: &iroh::PublicKey) -> PublicKey {
    PublicKey::from(*key.as_bytes())
}

pub fn to_iroh_secret_key(key: &PrivateKey) -> iroh::SecretKey {
    iroh::SecretKey::from_bytes(&key.to_bytes())
}

pub fn from_iroh_secret_key(key: &iroh::SecretKey) -> PrivateKey {
    PrivateKey::from(key.to_bytes())
}

// Re-export iroh networking types
pub use iroh::discovery::pkarr::dht::DhtDiscovery;
pub use iroh::endpoint::{Connection, SendStream};
pub use iroh::protocol::{AcceptError, ProtocolHandler, Router};
pub use iroh::{
    Endpoint, NodeAddr, NodeId, PublicKey as IrohPublicKey, SecretKey as IrohSecretKey,
};

// Re-export iroh-blobs types
pub use iroh_blobs::api::blobs::{BlobReader, BlobStatus as ApiBlobStatus, Blobs};
pub use iroh_blobs::api::downloader::{Downloader, Shuffled};
pub use iroh_blobs::api::Store;
pub use iroh_blobs::api::{ExportBaoError, RequestError};
pub use iroh_blobs::store::{fs::FsStore, mem::MemStore};
pub use iroh_blobs::{BlobFormat, BlobsProtocol, Hash, HashAndFormat, ALPN as BLOBS_ALPN};
