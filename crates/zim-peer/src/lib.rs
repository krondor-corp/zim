//! `zim-peer` — what you need to be a peer on the network: sync
//! orchestration **and** the concrete daemon-side impls of the traits
//! `zim-core` exposes.
//!
//! Server-side / native only (iroh-coupled). Browser / wasm clients
//! build directly against `zim-core` with their own `BlobStore` /
//! `VaultLog` impls.
//!
//! ## Layout
//!
//! Sync orchestration:
//! - [`messages`] — wire-level Request/Reply structs (`Head`,
//!   `Probe`, `Ancestor`, `Ping`, `ShareOffered`).
//! - [`effect::Effect`] — side-effects taxonomy for background work.
//! - [`coordinator::SyncCoordinator`] — vault open/sync entry points
//!   (`open_vault`, `sync_vault`, `apply_chain`) + log-only reply
//!   handlers + background effect runner.
//! - [`iroh_transport`] — iroh `ProtocolHandler` impl.
//! - [`relay_pull`] — hub-mirror log-only chain pull.
//! - [`chain`] — manifest-chain walk + ops merge primitives.
//!
//! Concrete impls:
//! - [`log::SqliteVaultLog`] / [`log::MemoryVaultLog`] — `VaultLog`
//!   impls.
//! - [`object_store`] — SQLite-indexed local/S3 blob backend bridged
//!   into `BlobsProvider`.
//! - [`object_store`] — SQLite-indexed local/S3 blob backend,
//!   bridged into `BlobsProvider`.
//! - [`peers`] — concrete `PeerStore` impls (trait lives in
//!   `zim_core::peers`).
//!
//! There's no peer-side vault wrapper anymore: [`Vault`] is a type
//! alias for [`zim_core::vault::Vault`] over the daemon's
//! [`BlobsProvider`]. Sync methods that need the iroh endpoint live
//! on [`SyncCoordinator`], which owns one.

pub mod chain;
pub mod coordinator;
pub mod effect;
pub mod iroh_transport;
pub mod log;
pub mod messages;
pub mod object_store;
pub mod peer;
pub mod peers;
pub mod relay_pull;
pub mod wire_protocol;

/// Daemon-side vault: the core vault over the iroh-blobs provider.
pub type Vault<L> = zim_core::vault::Vault<zim_core::blobs::BlobsProvider, L>;

pub use coordinator::{run_effects, DaemonInfo, MemoryPeerSender, SentMessage, SyncCoordinator};
pub use effect::Effect;
pub use iroh_transport::{IrohPeerSender, SyncProtocol, ALPN};
pub use log::{MemoryVaultLog, SqliteVaultLog};
pub use messages::{
    Ack, AncestorReply, AncestorRequest, HeadAdvanced, HeadReply, HeadRequest, PingRequest,
    PongReply, ProbeReply, ProbeRequest, ShareOffered,
};
pub use peer::{Discovery, Peer, PeerBuilder, VaultListing, VaultLookupError};
pub use peers::{MemoryPeerStore, PeerEntry, PeerStore, PeerStoreError};
pub use wire_protocol::{dispatch_request, PeerSender, WireReply, WireRequest};
pub use zim_core::vault::{VaultError, VaultLog, VaultLogError};
