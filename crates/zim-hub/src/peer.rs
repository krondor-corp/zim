//! The hub's embedded peer — its own definition, not the daemon's.
//!
//! The hub is a relay: it mirrors ciphertext + the vault log for its
//! tenants and speaks the same iroh sync protocol daemons do, but it
//! holds no shares and mounts nothing. So its peer is exactly:
//! identity key + sqlite log + the caller's blob store (S3/minio or
//! local) + [`HubAcceptPolicy`](crate::accept::HubAcceptPolicy).
//! No contacts book, no resolver, no FUSE — none of the daemon's
//! `ServiceState` baggage.

use std::path::Path;
use std::sync::Arc;

use zim_crypto::PrivateKey;
use zim_peer::{AcceptPolicy, BlobsProvider, Peer, SqliteVaultLog};

pub type HubPeer = Peer<SqliteVaultLog>;

#[derive(Debug, thiserror::Error)]
pub enum PeerBootError {
    #[error("identity: {0}")]
    Identity(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite log: {0}")]
    SqliteLog(String),
    #[error("peer build: {0}")]
    Build(String),
}

/// Load (or mint on first boot) the hub's persistent iroh identity —
/// the key `/.well-known/did.json` publishes and hosted devices dial.
async fn load_or_create_identity(data_dir: &Path) -> Result<PrivateKey, PeerBootError> {
    let path = data_dir.join("identity.key");
    if path.exists() {
        let hex = tokio::fs::read_to_string(&path).await?;
        PrivateKey::from_hex(hex.trim()).map_err(|e| PeerBootError::Identity(e.to_string()))
    } else {
        let secret = PrivateKey::generate();
        tokio::fs::write(&path, secret.to_hex()).await?;
        Ok(secret)
    }
}

/// Boot the relay peer rooted at `data_dir`.
pub async fn boot(
    data_dir: &Path,
    blobs: BlobsProvider,
    accept: Arc<dyn AcceptPolicy>,
) -> Result<HubPeer, PeerBootError> {
    tokio::fs::create_dir_all(data_dir).await?;
    let secret = load_or_create_identity(data_dir).await?;
    let log = SqliteVaultLog::new(&data_dir.join("log.sqlite"))
        .map_err(|e| PeerBootError::SqliteLog(e.to_string()))?;

    Peer::builder()
        .with_secret(secret)
        .with_log(log)
        .with_accept_policy(accept)
        .with_blobs(blobs)
        // pkarr DHT so daemons can dial the hub (and vice versa) by
        // pubkey alone.
        .with_pkarr_discovery()
        .with_info(zim_peer::DaemonInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            started_at: std::time::Instant::now(),
        })
        .build()
        .await
        .map_err(|e| PeerBootError::Build(e.to_string()))
}
