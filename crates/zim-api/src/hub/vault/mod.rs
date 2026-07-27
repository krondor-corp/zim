//! `/api/v0/vaults/{vault_id}/*` — one vault's mirror surface: its head
//! and its append-only log. (Ciphertext blobs are the vault-agnostic
//! [`super::blob`] — content-addressed, no vault id in the request.)
//!
//! Spoken by the browser wasm SDK (`zim-hub-wasm`, per-call JWT auth
//! through its own dispatcher) and serialized by the hub server. Daemons
//! sync vaults over iroh, not these routes.

pub mod head;
pub mod log;
pub mod write_head;

pub use head::{HeadRequest, HeadResponse};
pub use log::{LogEntry, LogQuery, LogRequest, LogResponse};
pub use write_head::{WriteHeadBody, WriteHeadRequest, WriteHeadResponse};
