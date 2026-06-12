//! Wire-level Request/Reply structs for the sync protocol.
//!
//! Three families, all serde-serializable so they can ride any
//! transport (iroh bidirectional streams, HTTP, in-memory test bus):
//!
//! - **Head** — "what's your current head for this vault?" One field
//!   each direction; the cheapest possible query.
//! - **Probe** — initiator-driven bisect: "do you have any of these
//!   `(height, link)` pairs in your log?" One round-trip handles the
//!   common case (recent divergence ≤ 2^sample_size versions).
//! - **Ancestor** — higher-level "find the common ancestor between us
//!   for this vault." The responder may handle this by running its own
//!   probe loop and returning the answer in one round-trip, or may
//!   reply with `Defer { sample }` asking the initiator to bisect.
//!
//! These types know nothing about transport. The coordinator owns the
//! send/receive plumbing.

use serde::{Deserialize, Serialize};

use zim_core::vault::{Head, VaultId};

// ─── Head ───────────────────────────────────────────────────────────

/// "What is your current head for vault `vault_id`?"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadRequest {
    pub vault_id: VaultId,
}

/// Reply to [`HeadRequest`]. `head` is `None` if the responder doesn't
/// know about this vault at all.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadReply {
    pub vault_id: VaultId,
    pub head: Option<Head>,
}

// ─── Probe ──────────────────────────────────────────────────────────

/// "Do you have any of these chain positions in your log? Reply
/// with the deepest match."
///
/// `sample` is built via [`zim_core::vault::VaultLog::exponential_sample`]
/// and should be in descending-by-height order. The responder scans it
/// and returns the first entry it shares.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeRequest {
    pub vault_id: VaultId,
    pub sample: Vec<Head>,
}

/// Reply to [`ProbeRequest`]. `highest` is `None` if no sample entry
/// is in our log (we either don't know this vault or diverged before
/// any sampled height).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeReply {
    pub vault_id: VaultId,
    pub highest: Option<Head>,
}

// ─── Ancestor ───────────────────────────────────────────────────────

/// "Tell me the common ancestor between your chain and mine for
/// vault `vault_id`, given my current head."
///
/// Higher-level than [`ProbeRequest`]: the initiator names only its
/// head and the responder decides how to answer (one-shot if they can
/// compute it locally, or by asking for a probe sample via
/// [`AncestorReply::NeedProbe`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AncestorRequest {
    pub vault_id: VaultId,
    pub initiator_head: Head,
}

/// Reply to [`AncestorRequest`].
///
/// There is deliberately no "divergent vaults" variant: vault ids
/// derive from the genesis hash, so two verified chains for the same
/// id always intersect — at worst at genesis. A chain that doesn't
/// is *not this vault* and fails verification upstream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AncestorReply {
    /// We computed the ancestor and here it is.
    Found { vault_id: VaultId, ancestor: Head },
    /// We don't know this vault.
    NotFound { vault_id: VaultId },
    /// We can't compute the ancestor from the initiator's head alone
    /// (e.g. our log doesn't have a fast index). Send us a probe
    /// sample and we'll answer with the deepest match.
    NeedProbe { vault_id: VaultId },
}

impl AncestorReply {
    pub fn vault_id(&self) -> VaultId {
        match self {
            Self::Found { vault_id, .. }
            | Self::NotFound { vault_id }
            | Self::NeedProbe { vault_id } => *vault_id,
        }
    }
}

// ─── Ping ───────────────────────────────────────────────────────────

/// "Are you there? Tell me about yourself." Cheap connectivity probe;
/// the reply carries identity + version + uptime so the caller can
/// render `peers ping <nick>` output without round-tripping anything
/// else.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PingRequest;

/// Reply payload for [`PingRequest`]. `version` is whatever string
/// the responder chose to advertise — typically `BuildInfo::version`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PongReply {
    /// Hex of the responder's pubkey — lets the caller sanity-check
    /// against the address-book entry they thought they were dialing.
    pub peer_id: String,
    pub version: String,
    pub uptime_secs: u64,
}

// ─── Share offer ────────────────────────────────────────────────────

/// "I just added you to the shares of this vault — here's its head
/// so you can pull the chain." Fire-and-forget push: receiver
/// answers with a bare ack and then bootstraps the vault locally on
/// its own time (manifest walk → log population → registry insert).
///
/// v1 is "spam on share" — alice's daemon sends this every time her
/// `share` op adds a recipient, no opt-in handshake. We accept the
/// risk that anyone could announce a vault to us; later versions can
/// gate this behind a known-peers list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareOffered {
    pub vault_id: VaultId,
    pub head: Head,
}

// ─── Head advanced (push) ───────────────────────────────────────────

/// "I just advanced to this head on `vault_id` — come pull."
/// Fire-and-forget push: receiver acks and submits an
/// `Effect::PullFromPeer` on its own time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadAdvanced {
    pub vault_id: VaultId,
    pub head: Head,
}

// ─── Ack ────────────────────────────────────────────────────────────

/// Bare acknowledgement — wire-level "I got it." Carries no payload;
/// the unit struct exists only so every `WireReply` variant has a
/// uniformly-shaped body (lets the `wire_protocol!` macro stay
/// special-case-free).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Ack;
