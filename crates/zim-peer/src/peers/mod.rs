//! Concrete [`PeerStore`] impls.
//!
//! The trait + row types live in [`zim_core::peers`] — same split as
//! [`VaultLog`](zim_core::vault::VaultLog): core owns the storage
//! abstraction, this crate owns the daemon-side impls. The `zim`
//! daemon's `peers.toml`-backed store lives in
//! `crates/zim/src/peers.rs`; the in-memory one here backs unit
//! tests.

mod memory;
pub use memory::MemoryPeerStore;

pub use zim_core::peers::{PeerEntry, PeerStore, PeerStoreError};
