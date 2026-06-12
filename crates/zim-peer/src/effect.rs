//! The side-effects taxonomy.
//!
//! All effects are *background work*. Peer-message reply handlers
//! never wait on effects — they answer from the log and (optionally)
//! submit an effect into the queue. The background runner picks them
//! up and dispatches via [`SyncCoordinator::execute`](crate::SyncCoordinator::execute).
//!
//! Three live variants today: [`Self::PullFromPeer`],
//! [`Self::AnnounceHead`], [`Self::OfferShare`]. Earlier drafts
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

    /// Tell a peer we advanced. Push, no reply expected.
    AnnounceHead {
        peer_id: PublicKey,
        vault_id: VaultId,
        head: Head,
    },

    /// Tell a peer "I just added you to this vault's shares — here's
    /// where I am, come pull it." One-way push fired by the HTTP
    /// `share` handler after `vault.add_share + save`. The receiver
    /// bootstraps the vault into its local registry. See
    /// `docs/research/optimistic-share-acceptance.md` for the v2
    /// non-blocking variant.
    OfferShare {
        peer_id: PublicKey,
        vault_id: VaultId,
        head: Head,
    },
}
