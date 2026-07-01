//! Trusted-contact share population.
//!
//! Share population is daemon orchestration, not a sync-protocol
//! concern: read the contacts book, resolve DIDs, mutate the vault, and
//! announce. The sync layer only transports the resulting manifest.

use zim_did::DidResolver;
use zim_peer::{Peer, PeerStore, SqlitePeerStore, VaultLog};

/// Outcome of a [`reconcile_trusted`] sweep.
#[derive(Debug, Clone, Default)]
pub struct ReconcileReport {
    /// Vaults this peer authored and therefore considered.
    pub vaults_scanned: usize,
    /// Vaults that gained at least one share (saved + announced).
    pub vaults_updated: usize,
    /// Total shares added across all vaults this run.
    pub shares_added: usize,
}

/// Fold every **trusted** contact's resolved device keys into the vaults
/// this peer *authored*.
///
/// The auto-share sweep: trusted contacts (your own devices from `hub
/// peers sync`, plus anyone you `peers add --trust`) are granted access
/// to every vault you own, in one pass. Idempotent — a vault already
/// holding all of a contact's keys is left untouched (no save, no chain
/// advance, no announce), so it's safe to call on `vault create`, after
/// `hub peers sync`, or on a timer.
///
/// Scope and safety:
/// - Only vaults **authored by this peer** are touched. Being a mere
///   shareholder of someone else's vault doesn't license re-sharing it.
/// - **Never removes** a share. A contact that fails to resolve is
///   logged and skipped — a transient DNS/HTTP failure must not silently
///   revoke a device.
/// - A vault that advances gets its new head announced, so the freshly
///   added devices pull it (a browser key via its hub, a daemon direct).
pub async fn reconcile_trusted<L>(
    peer: &Peer<L>,
    contacts: &SqlitePeerStore,
    resolver: &dyn DidResolver,
) -> anyhow::Result<ReconcileReport>
where
    L: VaultLog + Clone + 'static,
    L::Error: std::error::Error + Send + Sync + 'static,
{
    let me = peer.id();

    // Resolve every trusted contact to its device reaches once; a
    // contact that fails to resolve is logged and skipped, not fatal.
    let trusted = contacts
        .list_trusted()
        .await
        .map_err(|e| anyhow::anyhow!("list trusted contacts: {e}"))?;
    let mut reaches: Vec<zim_did::Reach> = Vec::new();
    for entry in trusted {
        // A contact's stored `via` (the hub, for a browser/web device)
        // overrides the `did:key` default of "dial directly" — so the
        // resulting share is reached through the hub, not by trying to
        // dial the browser. Resolve it to a key once.
        let contact_via = match &entry.via {
            Some(v) => match zim_did::resolve_pubkey(v, resolver).await {
                Ok(pk) => Some((v.clone(), pk)),
                Err(e) => {
                    tracing::warn!(nick = %entry.nick, "reconcile: resolve via failed: {e}");
                    None
                }
            },
            None => None,
        };
        match zim_did::resolve_reaches(&entry.identity, resolver).await {
            Ok(rs) => reaches.extend(rs.into_iter().map(|mut r| {
                if r.via.is_none() {
                    r.via = contact_via.clone();
                }
                r
            })),
            Err(e) => tracing::warn!(nick = %entry.nick, "reconcile: resolve failed: {e}"),
        }
    }

    let mut report = ReconcileReport::default();
    let ids = peer
        .coord()
        .log()
        .list_vaults()
        .await
        .map_err(anyhow::Error::from)?;
    for id in ids {
        let mut vault = match peer.vault(id).await {
            Ok(v) => v,
            Err(e) => {
                // A relay/hub mirror holds ciphertext but no share, so it
                // can't open — expected, skip quietly.
                tracing::debug!(vault_id = %id, "reconcile: open skipped: {e}");
                continue;
            }
        };
        if vault.manifest().author() != &me {
            continue;
        }
        report.vaults_scanned += 1;

        let mut added = 0usize;
        for reach in &reaches {
            if reach.client == me || vault.manifest().get_share(&reach.client).is_some() {
                continue;
            }
            vault
                .add_reach(reach.clone())
                .map_err(|e| anyhow::anyhow!("add share to {id}: {e}"))?;
            added += 1;
        }
        if added == 0 {
            continue;
        }
        vault
            .save()
            .await
            .map_err(|e| anyhow::anyhow!("save {id}: {e}"))?;
        let head = vault
            .head()
            .await
            .map_err(|e| anyhow::anyhow!("head {id}: {e}"))?;
        peer.announce_head(&vault, head).await;
        report.vaults_updated += 1;
        report.shares_added += added;
        tracing::info!(vault_id = %id, added, "reconcile: shared with trusted contacts");
    }
    Ok(report)
}
