#[cfg(feature = "fuse")]
use std::sync::Arc;

#[cfg(feature = "fuse")]
use tokio::sync::RwLock;
use url::Url;

use crate::blobs::{Blobs, BlobsSetupError};
use crate::database::{Database, DatabaseSetupError};
#[cfg(feature = "fuse")]
use crate::fuse::{MountManager, MountManagerConfig};
use crate::service_config::Config;
use crate::sync_provider::{JobReceiver, QueuedSyncConfig, QueuedSyncProvider};

use zim_crypto::PrivateKey;
use zim_protocol::{Peer, PeerBuilder};

/// Main service state - orchestrates all components
#[derive(Clone)]
pub struct State {
    database: Database,
    peer: Peer<Database>,
    #[cfg(feature = "fuse")]
    mount_manager: Arc<RwLock<Option<MountManager>>>,
}

impl State {
    /// Construct service state from config.
    ///
    /// Returns both the state and a [`JobReceiver`] for the sync provider's
    /// worker queue. **The caller is responsible for spawning
    /// [`crate::sync_provider::run_worker`] with a shutdown receiver** —
    /// without that, sync jobs queue up but never execute, and (more
    /// importantly) any worker spawned here would not see the daemon
    /// shutdown signal and would leak past process drain (T-007a-C).
    pub async fn from_config(config: &Config) -> Result<(Self, JobReceiver), StateSetupError> {
        // 1. Setup database
        let sqlite_database_url = match config.sqlite_path {
            Some(ref path) => {
                // check that the path exists
                if !path.exists() {
                    return Err(StateSetupError::DatabasePathDoesNotExist);
                }
                // parse the path into a URL
                Url::parse(&format!("sqlite://{}", path.display()))
                    .map_err(|_| StateSetupError::InvalidDatabaseUrl)
            }
            // otherwise just set up an in-memory database
            None => Url::parse("sqlite::memory:").map_err(|_| StateSetupError::InvalidDatabaseUrl),
        }?;
        tracing::info!("Database URL: {:?}", sqlite_database_url);
        let database = Database::connect(&sqlite_database_url)?;

        // 2. Setup node secret
        let node_secret = config
            .node_secret
            .clone()
            .unwrap_or_else(PrivateKey::generate);

        // 3. Setup blobs store using the new blobs module
        tracing::debug!("ServiceState::from_config - loading blobs store");
        let blobs =
            Blobs::setup(&config.blob_store, &config.jax_dir, config.max_import_size).await?;
        tracing::debug!("ServiceState::from_config - blobs store loaded successfully");

        // 4. Build peer from the database as the log provider
        // TODO: Make queue size configurable via config

        // Create sync provider with worker
        let (sync_provider, job_receiver) = QueuedSyncProvider::new(QueuedSyncConfig::default());

        let mut peer_builder = PeerBuilder::new()
            .with_sync_provider(std::sync::Arc::new(sync_provider))
            .log_provider(database.clone())
            .blobs_store(blobs.into_inner())
            .secret_key(node_secret.clone());

        if let Some(addr) = config.node_listen_addr {
            peer_builder = peer_builder.socket_address(addr);
        }

        let peer = peer_builder.build().await;

        // Log the bound addresses
        let bound_addrs = peer.endpoint().bound_sockets();
        tracing::info!("Dir id: {} (with JAX protocol)", peer.id());
        tracing::info!("Peer listening on: {:?}", bound_addrs);

        // NOTE (T-007a-C): we do NOT spawn the sync worker here. Spawning it
        // bare leaked the JoinHandle and the worker lived past daemon
        // shutdown. The worker is now the caller's responsibility — they get
        // the JobReceiver back via the `(Self, JobReceiver)` return and
        // spawn `crate::sync_provider::run_worker(peer, stream, shutdown_rx)`
        // under their lifecycle layer with the daemon's shutdown signal.

        // Create the initial state
        let state = Self {
            database: database.clone(),
            peer: peer.clone(),
            #[cfg(feature = "fuse")]
            mount_manager: Arc::new(RwLock::new(None)),
        };

        // Initialize mount manager with fuse feature
        #[cfg(feature = "fuse")]
        {
            let mount_manager = MountManager::new(
                database,
                peer,
                MountManagerConfig {
                    api_port: config.api_port,
                    ..MountManagerConfig::default()
                },
            );
            *state.mount_manager.write().await = Some(mount_manager);
        }

        Ok((state, job_receiver))
    }

    pub fn peer(&self) -> &Peer<Database> {
        &self.peer
    }

    pub fn node(&self) -> &Peer<Database> {
        // Alias for backwards compatibility
        &self.peer
    }

    pub fn database(&self) -> &Database {
        &self.database
    }

    /// Get the mount manager (only available with fuse feature)
    #[cfg(feature = "fuse")]
    pub fn mount_manager(&self) -> &Arc<RwLock<Option<MountManager>>> {
        &self.mount_manager
    }
}

impl AsRef<Peer<Database>> for State {
    fn as_ref(&self) -> &Peer<Database> {
        &self.peer
    }
}

impl AsRef<Database> for State {
    fn as_ref(&self) -> &Database {
        self.database()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StateSetupError {
    #[error("Database path does not exist")]
    DatabasePathDoesNotExist,
    #[error("Database setup error")]
    DatabaseSetupError(#[from] DatabaseSetupError),
    #[error("Invalid database URL")]
    InvalidDatabaseUrl,
    #[error("Blobs setup error: {0}")]
    BlobsSetupError(#[from] BlobsSetupError),
}
