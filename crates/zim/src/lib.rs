//! `zim` — CLI + daemon for an encrypted, peer-to-peer vault
//! filesystem.
//!
//! Crate layout mirrors `_zim-peer`:
//!
//! - [`cli`] — clap args + `Op` trait + per-command ops + UI helpers
//! - [`context`] — XDG-aware paths, persistent config, typed
//!   per-command contexts
//! - [`http_server`] — axum API + health, plus the typed
//!   [`http_server::api::client::ApiClient`]
//! - [`service_config`] / [`service_state`] — daemon runtime
//! - [`version`] — `BuildInfo` populated by `build.rs`
//!
//! The `command_enum!` macro at the crate root is exported from
//! [`cli::op`] for `main.rs` to wire up the top-level `Command` enum.

pub mod cli;
pub mod context;
pub mod http_server;
pub mod hub_jwt;
pub mod peers;
pub mod service_config;
pub mod service_state;
pub mod version;

// Re-export so handler files can use `crate::ServiceState` (matches
// the `_zim-peer` convention).
pub use service_state::ServiceState;

use crate::cli::ops;

crate::command_enum! {
    (Init,    ops::Init),
    (Login,   ops::Login),
    #[command(subcommand)] (Daemon,  ops::Daemon),
    (Id,      ops::Id),
    (Health,  ops::Health),
    (Version, ops::Version),
    #[command(subcommand)] (Peers,   ops::Peers),
    #[command(subcommand)] (Vaults,  ops::Vaults),
    (Vault,   ops::Vault),
}
