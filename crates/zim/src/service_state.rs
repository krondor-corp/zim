//! Daemon-side runtime state — held by every axum handler via `State`.
//!
//! Owns the `zim_peer::Peer<SqliteVaultLog>` plus the contacts book, the
//! DID resolver, and the resolved data dir. Vaults are tracked by the
//! inner `SyncCoordinator`'s registry
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
use zim_did::{DidResolver, HttpDidResolver};
use zim_peer::SqliteVaultLog;
use zim_peer::{AcceptPolicy, Peer, SqlitePeerStore, VaultLookupError};

use crate::accept::ContactsAcceptPolicy;
use crate::context::paths;

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
    peer: Peer<SqliteVaultLog>,
    /// The contacts address book — `nick → DID` with a `trusted` flag,
    /// in the daemon's `log.sqlite`. Shared with the acceptance policy
    /// (same handle), so HTTP handlers and the gate see one table.
    peers: SqlitePeerStore,
    /// DID resolver. Lives here, not on the peer: the sync protocol owns
    /// no DID resolution. Used by the share handler, reconcile, and the
    /// acceptance policy.
    resolver: Arc<dyn DidResolver>,
    home: PathBuf,
    /// FUSE mount lifecycle. Shared (mutated through interior locks), so it's
    /// behind an `Arc` to keep `ServiceState: Clone`.
    #[cfg(feature = "fuse")]
    mounts: Arc<crate::mount::MountManager>,
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
        Self::boot_with_blobs(home, blobs, None).await
    }

    /// Boot with a caller-supplied blob store and (optionally) a custom
    /// [`AcceptPolicy`]. The hub uses this to plug in an S3 blob store
    /// (minio in dev) *and* a `user_peers`-backed acceptance policy;
    /// `None` falls back to the daemon's contacts-backed policy.
    pub async fn boot_with_blobs(
        home: &Path,
        blobs: BlobsProvider,
        accept: Option<Arc<dyn AcceptPolicy>>,
    ) -> Result<Self, StateError> {
        let secret = load_or_create_identity(home).await?;
        let log = SqliteVaultLog::new(&paths::log_file(home))
            .map_err(|e| StateError::SqliteLog(e.to_string()))?;
        // Contacts book lives in the same `log.sqlite` (one migration
        // set). Opened separately from `log` — distinct connections to
        // the same file, which WAL handles fine.
        let peers = SqlitePeerStore::open(&paths::log_file(home))
            .map_err(|e| StateError::SqliteLog(e.to_string()))?;

        // `allow_http` lets the dev/loopback path resolve hubs on
        // `http://127.0.0.1:…` without TLS. Production hubs use
        // HTTPS; harmless in dev because resolver is only ever
        // called for did:web inputs.
        let resolver: Arc<dyn DidResolver> = Arc::new(HttpDidResolver::new().with_allow_http(true));

        // Inbound acceptance: the daemon gates new vaults on its
        // contacts; the hub passes its own `user_peers`-backed policy.
        let accept = accept.unwrap_or_else(|| {
            Arc::new(ContactsAcceptPolicy::new(peers.clone(), resolver.clone()))
                as Arc<dyn AcceptPolicy>
        });

        let peer = Peer::builder()
            .with_secret(secret)
            .with_log(log)
            .with_accept_policy(accept)
            .with_blobs(blobs)
            // Without discovery, peers can't dial each other by
            // pubkey alone — the announce effect would fail with
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

        #[cfg(feature = "fuse")]
        let mounts = Arc::new(crate::mount::MountManager::new(
            peer.clone(),
            tokio::runtime::Handle::current(),
            home,
        ));

        Ok(Self {
            peer,
            peers,
            resolver,
            home: home.to_path_buf(),
            #[cfg(feature = "fuse")]
            mounts,
        })
    }

    pub fn peer(&self) -> &Peer<SqliteVaultLog> {
        &self.peer
    }

    /// The DID resolver. Used by the share handler, reconcile, and the
    /// acceptance policy — DID resolution is a daemon concern, not the
    /// sync protocol's.
    pub fn resolver(&self) -> &Arc<dyn DidResolver> {
        &self.resolver
    }

    /// The contacts address book (`nick → DID`, with a `trusted` flag).
    /// Backed by the daemon's `log.sqlite`.
    pub fn peers(&self) -> &SqlitePeerStore {
        &self.peers
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    /// The FUSE mount manager (daemon built with `--features fuse`).
    #[cfg(feature = "fuse")]
    pub fn mounts(&self) -> &Arc<crate::mount::MountManager> {
        &self.mounts
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
