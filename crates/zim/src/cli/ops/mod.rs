//! Top-level CLI operations.

pub mod daemon;
pub mod health;
pub mod id;
pub mod init;
pub mod login;
pub mod peers;
pub mod vault;
pub mod vaults;
pub mod version;

pub use daemon::Daemon;
pub use health::Health;
pub use id::Id;
pub use init::Init;
pub use login::Login;
pub use peers::Peers;
pub use vault::Vault;
pub use vaults::Vaults;
pub use version::Version;
