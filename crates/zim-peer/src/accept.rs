//! Inbound acceptance hook.
//!
//! The sync protocol owns no address book. When a push arrives it asks
//! one question — *"should I accept this?"* — through [`AcceptPolicy`].
//! Where the answer comes from (a daemon's contacts table, the hub's
//! multi-user `user_peers` registry, a test stub) is the binary's
//! business, not the protocol's.
//!
//! The hook fires on **every** `HeadAdvanced`, not just unknown vaults:
//! a first-time share to a new recipient lands as an *advance* to an
//! existing vault, so a policy that only ran for new vaults would miss
//! it. [`IncomingSync::known_vault`] lets a policy fast-path known
//! vaults if it wants to.

use async_trait::async_trait;

use zim_core::vault::VaultId;
use zim_crypto::PublicKey;

/// One inbound push, as seen by an [`AcceptPolicy`].
#[derive(Debug, Clone)]
pub struct IncomingSync {
    /// Who dialed us and pushed.
    pub sender: PublicKey,
    /// The shareholder client this push is *for*. Equal to the receiver
    /// for a direct peer; a hosted device (e.g. a browser key) for a
    /// relay, where the receiver (the hub) is not the recipient.
    pub recipient: PublicKey,
    pub vault_id: VaultId,
    /// Whether we already mirror this vault. A policy may fast-path
    /// known vaults rather than re-gating every advance.
    pub known_vault: bool,
}

/// Decides whether to accept an inbound vault push. Supplied to the
/// [`SyncCoordinator`](crate::SyncCoordinator) at construction; the
/// protocol holds it as a trait object and never inspects an address
/// book itself.
#[async_trait]
pub trait AcceptPolicy: Send + Sync + 'static {
    /// Accept this push? Returning `false` drops it silently.
    async fn accept_sync(&self, sync: &IncomingSync) -> bool;
}

/// Accept every push. The default — right for a single-peer setup or a
/// test, where there's no untrusted sender to gate against.
#[derive(Debug, Clone, Copy, Default)]
pub struct AcceptAll;

#[async_trait]
impl AcceptPolicy for AcceptAll {
    async fn accept_sync(&self, _sync: &IncomingSync) -> bool {
        true
    }
}
