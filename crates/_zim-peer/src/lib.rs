#![allow(clippy::result_large_err)]
// Service modules (daemon functionality)
pub mod backup_sync;
pub(crate) mod blobs;
pub mod clone_state;
pub(crate) mod database;
#[cfg(feature = "fuse")]
pub mod fuse;
pub mod http_server;
pub mod process;
pub mod service_config;
pub mod service_state;
pub mod sync_provider;
pub mod version;

// App state (configuration, paths)
pub mod state;

// Re-exports for consumers (Tauri, etc.)
pub use database::Database;
pub use process::{spawn_peer_services, spawn_service, start_service};
pub use service_config::Config as ServiceConfig;
pub use service_state::State as ServiceState;
pub use state::{AppConfig, AppState, BlobStoreConfig, StateError};
pub use zim_runtime::ShutdownHandle;

// Re-exports for mount management
pub use database::models::BucketStatus;
pub use database::models::FuseMount;
pub use database::types::MountStatus;
pub use version::build_info;
