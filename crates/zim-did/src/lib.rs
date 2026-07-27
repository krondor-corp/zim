//! DIDs (Decentralized Identifiers) for Zim.
//!
//! The point of going DID-first is to **separate logical identity
//! from raw key material**. Before this crate, every "peer" in the
//! system was a raw [`PublicKey`] — fine for a single daemon, broken
//! for users with multiple devices and hubs that serve as rendezvous
//! infrastructure. With DIDs:
//!
//! - A **daemon** is a `did:key:<multibase>` — self-describing, zero
//!   network resolution. The DID literally encodes the pubkey.
//! - A **hub** is a `did:web:hub.example.com` — resolves to a DID
//!   document over HTTPS, signed by the hub operator.
//! - A **user** is a `did:web:hub.example.com:u:alice` — resolves to
//!   a DID document listing multiple verification methods, one per
//!   device or browser.
//!
//! See `docs/research/hub-revival.md` for the architectural plan and
//! the role this crate plays in it.
//!
//! ## Scope
//!
//! - Type-level: [`Did`] (parse + display + [`Did::pubkey`] /
//!   [`Did::from_key`]) — THE identity type `Share` and `PeerEntry`
//!   carry; [`DidMethod`].
//! - `did:key` codec — ed25519 over multibase + multicodec prefix.
//! - `did:web` parsing. Resolution is trait-shaped ([`DidResolver`]);
//!   the reqwest-backed `HttpDidResolver` lives in
//!   `zim-api::hub::resolver` so this crate carries no HTTP stack.
//!   Tests typically use [`StaticResolver`].

mod did;
mod did_key;
mod document;
mod resolver;

pub use did::{Did, DidError, DidMethod};
pub use did_key::{did_key_decode, did_key_encode};
pub use document::{DidDocument, VerificationMethod};
pub use resolver::{
    did_web_url, pick_pubkey, resolve_pubkey, resolve_reaches, DidResolver, Reach, ResolveError,
    StaticResolver,
};
