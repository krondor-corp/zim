//! Top-level CLI operations.

#[cfg(debug_assertions)]
pub mod clean;
pub mod daemon;
pub mod health;
#[cfg(feature = "hub")]
pub mod hub;
pub mod id;
pub mod init;
pub mod mount;
pub mod peers;
pub mod vault;
pub mod vaults;
pub mod version;

#[cfg(debug_assertions)]
pub use clean::Clean;
pub use daemon::Daemon;
pub use health::Health;
#[cfg(feature = "hub")]
pub use hub::Hub;
pub use id::Id;
pub use init::Init;
pub use mount::Mount;
pub use peers::Peers;
pub use vault::Vault;
pub use vaults::Vaults;
pub use version::Version;
