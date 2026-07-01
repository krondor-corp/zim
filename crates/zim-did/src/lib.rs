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
//! - Type-level: [`Did`] (parse + display), [`Identity`] (the enum
//!   that `Share` / `Relay` and `PeerEntry` carry instead of a raw
//!   pubkey), [`DidMethod`].
//! - `did:key` codec — ed25519 over multibase + multicodec prefix.
//! - `did:web` parsing + `HttpDidResolver` (reqwest-backed, cached;
//!   behind the default `http-resolver` feature).
//!
//! Anything that needs a live pubkey from a `did:web` either calls
//! the convenience `HttpDidResolver` or constructs its own resolver
//! by implementing [`DidResolver`]. Tests typically use
//! [`StaticResolver`].

use zim_crypto::PublicKey;

mod did;
mod did_key;
mod document;
#[cfg(feature = "http-resolver")]
mod http_resolver;
mod identity;
mod resolver;

pub use did::{Did, DidError, DidMethod};
pub use did_key::{did_key_decode, did_key_encode};
pub use document::{DidDocument, VerificationMethod};
#[cfg(feature = "http-resolver")]
pub use http_resolver::HttpDidResolver;
pub use identity::{Identity, IdentityError};
pub use resolver::{
    did_web_url, pick_pubkey, resolve_pubkey, resolve_reaches, DidResolver, Reach, ResolveError,
    StaticResolver,
};

/// Convenience: build an [`Identity`] from a daemon's ed25519
/// pubkey. Sugar for `Identity::Key(pk)` / the `did:key` path.
pub fn key_identity(pk: PublicKey) -> Identity {
    Identity::Key(pk)
}
