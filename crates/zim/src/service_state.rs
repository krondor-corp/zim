//! Daemon-side runtime state — held by every axum handler via `State`.
//!
//! Owns the `zim_peer::Peer<SqliteVaultLog, TomlPeerStore>` plus the resolved data
//! dir. Vaults are tracked by the inner `SyncCoordinator`'s registry
//! and persisted in the `SqliteVaultLog`; the daemon doesn't keep a
//! separate "current vault" notion. Per-vault handlers take their
//! target via the URL (`/api/v0/vault/:vault_id/...`) and the
//! [`VaultHandle`](crate::http_server::api::v0::vault::extractor)
//! extractor resolves it to an open vault.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use zim_core::blobs::BlobsProvider;
use zim_crypto::PrivateKey;
use zim_did::HttpDidResolver;
use zim_peer::SqliteVaultLog;
use zim_peer::{Peer, VaultLookupError};

use crate::context::paths;
use crate::peers::TomlPeerStore;

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("sqlite log: {0}")]
    SqliteLog(String),
    #[error("blobs store: {0}")]
    Blobs(String),
    #[error("peer build: {0}")]
    PeerBuild(String),
    #[error("identity parse: {0}")]
    Identity(String),
}

/// Map a [`VaultLookupError`] onto an HTTP response. `NotFound` is a
/// recoverable domain condition (404); everything in `Backing` is
/// surfaced as a 500 — the message preserves the anyhow chain.
///
/// Lives here rather than `impl IntoResponse for VaultLookupError`
/// because the error type is foreign (it lives in `zim-peer`).
/// Handlers using `#[from] VaultLookupError` on their local error
/// enums delegate via this function in their own `IntoResponse`
/// arm.
pub fn vault_lookup_response(e: VaultLookupError) -> Response {
    match e {
        VaultLookupError::NotFound(_) => (StatusCode::NOT_FOUND, e.to_string()).into_response(),
        VaultLookupError::Backing(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

#[derive(Clone)]
pub struct ServiceState {
    peer: Peer<SqliteVaultLog, TomlPeerStore>,
    home: PathBuf,
}

impl ServiceState {
    /// Boot a fully-wired peer rooted at `home`, with the default
    /// filesystem blob store under `<home>/blobs/`.
    pub async fn boot(home: &Path) -> Result<Self, StateError> {
        let blobs_path = paths::blobs_dir(home);
        tokio::fs::create_dir_all(&blobs_path).await?;
        let blobs = BlobsProvider::legacy_fs(&blobs_path)
            .await
            .map_err(|e| StateError::Blobs(e.to_string()))?;
        Self::boot_with_blobs(home, blobs).await
    }

    /// Boot with a caller-supplied blob store. The hub uses this to
    /// plug in `zim_peer::object_store::s3_provider` (minio in dev,
    /// S3 in prod) instead of the local filesystem store.
    pub async fn boot_with_blobs(home: &Path, blobs: BlobsProvider) -> Result<Self, StateError> {
        let secret = load_or_create_identity(home).await?;
        let log = SqliteVaultLog::new(&paths::log_file(home))
            .map_err(|e| StateError::SqliteLog(e.to_string()))?;

        // `allow_http` lets the dev/loopback path resolve hubs on
        // `http://127.0.0.1:…` without TLS. Production hubs use
        // HTTPS; harmless in dev because resolver is only ever
        // called for did:web inputs.
        let resolver = Arc::new(HttpDidResolver::new().with_allow_http(true));

        let peers = TomlPeerStore::new(home.to_path_buf());
        let peer = Peer::builder()
            .with_secret(secret)
            .with_log(log)
            .with_peers(peers)
            .with_blobs(blobs)
            .with_resolver(resolver)
            // Without discovery, peers can't dial each other by
            // pubkey alone — the share-offer effect would fail with
            // "connect to peer …". pkarr DHT gets us peer→addr
            // resolution over the public network.
            .with_pkarr_discovery()
            // Identity reported in `Pong` replies: BuildInfo's version
            // + this process's start instant. `peers ping` will then
            // show how long the remote has been up.
            .with_info(zim_peer::DaemonInfo {
                version: crate::version::build_info().version.clone(),
                started_at: std::time::Instant::now(),
            })
            .build()
            .await
            .map_err(|e| StateError::PeerBuild(e.to_string()))?;

        Ok(Self {
            peer,
            home: home.to_path_buf(),
        })
    }

    pub fn peer(&self) -> &Peer<SqliteVaultLog, TomlPeerStore> {
        &self.peer
    }

    pub fn home(&self) -> &Path {
        &self.home
    }
}

async fn load_or_create_identity(home: &Path) -> Result<PrivateKey, StateError> {
    let path = paths::identity_file(home);
    if path.exists() {
        let hex = tokio::fs::read_to_string(&path).await?;
        PrivateKey::from_hex(hex.trim()).map_err(|e| StateError::Identity(e.to_string()))
    } else {
        let secret = PrivateKey::generate();
        tokio::fs::write(&path, secret.to_hex()).await?;
        Ok(secret)
    }
}
