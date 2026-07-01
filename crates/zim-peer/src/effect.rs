//! The side-effects taxonomy.
//!
//! All effects are *background work*. Peer-message reply handlers
//! never wait on effects — they answer from the log and (optionally)
//! submit an effect into the queue. The background runner picks them
//! up and dispatches via [`SyncCoordinator::execute`](crate::SyncCoordinator::execute).
//!
//! Two live variants today: [`Self::PullFromPeer`] and
//! [`Self::AnnounceHead`]. Earlier drafts
//! carried Vault-mutation variants (`Add` / `Mkdir` / `Rm` / `Save` /
//! `MergeWith`) and infra ones (`ApplyRemoteChain`, `DownloadBlobs`,
//! `Log`) but every mutation path the daemon actually has calls
//! `vault.X` directly inline — the queue detour was never wired up.
//! Add a variant back when there's a concrete producer needing async
//! background scheduling (Apalis-style background-job adoption would
//! be the trigger).

use zim_crypto::PublicKey;

use zim_core::vault::{Head, VaultId};

/// All side effects in the sync layer flow through this type.
#[derive(Debug, Clone)]
pub enum Effect {
    /// Pull from a specific peer end-to-end:
    /// `HeadRequest` → (compare heights) → `ProbeRequest` (build
    /// sample, learn ancestor) → chain download + merge. One Effect,
    /// three round-trips inside the runner.
    PullFromPeer {
        vault_id: VaultId,
        peer_id: PublicKey,
    },

    /// Tell `peer_id` we advanced — push, no reply expected. The sole
    /// notify mechanism: the receiver turns it into a `PullFromPeer`,
    /// accepting per its [`AcceptPolicy`](crate::AcceptPolicy) if the
    /// vault is new, or fast-forwarding if known. `recipient` is the
    /// shareholder this push serves — `peer_id` itself for a direct
    /// share, the hosted client when `peer_id` is its relay.
    AnnounceHead {
        peer_id: PublicKey,
        vault_id: VaultId,
        // Boxed: `Head` dwarfs the other variant, and an effect is moved
        // through an mpsc queue, so keep the enum small.
        head: Box<Head>,
        recipient: PublicKey,
    },
}
