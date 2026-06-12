//! Log-only chain pull for relay-mode peers (hubs, future browser
//! mirrors).
//!
//! Mirrors what `SyncCoordinator::apply_chain` does, but without
//! constructing a `Vault` (and therefore without requiring a `Share`
//! on the manifest or any decryption). Walks the chain by re-decoding
//! manifest blobs and following their `previous` links; for each
//! step it appends the entry to the log and pulls the pinned content
//! blobs.
//!
//! Used by `SyncCoordinator::pull_from_peer` when the local peer
//! doesn't hold a share for the vault — the hub-mirror path.

use zim_core::blobs::{BlobStore, BlobsProvider};
use zim_core::fs::Manifest;
use zim_core::iroh::{Downloader, Endpoint, Hash, Shuffled};
use zim_core::linked_data::Link;
use zim_core::vault::{Head, VaultId, VaultLog};
use zim_crypto::PublicKey;

/// Walk the chain backward from `target_link`, appending each
/// (manifest, link) into `log` and downloading every pinned content
/// hash. No decryption — the manifest blob is plaintext DAG-CBOR.
///
/// `ancestor` (when supplied) bounds the walk: if we encounter it,
/// we stop. Pass `None` to fetch from genesis (first-contact
/// mirroring).
#[allow(clippy::too_many_arguments)]
pub async fn apply_chain_log_only<L: VaultLog>(
    vault_id: VaultId,
    target: Head,
    ancestor: Option<Link>,
    peer_id: PublicKey,
    blobs: &BlobsProvider,
    log: &L,
    endpoint: &Endpoint,
) -> anyhow::Result<()> {
    // No-op if we already have this height.
    if log
        .exists(vault_id)
        .await
        .map_err(|e| anyhow::anyhow!("log.exists({vault_id}): {e}"))?
    {
        let ours = log
            .head(vault_id, None)
            .await
            .map_err(|e| anyhow::anyhow!("log.head({vault_id}): {e}"))?;
        if ours.height >= target.height {
            return Ok(());
        }
    }

    // Pre-download the head manifest blob so the loop below can
    // decode it.
    let target_link = target.link;
    let head_hash = target_link.hash();
    download_hash(head_hash, &[peer_id], blobs, endpoint)
        .await
        .map_err(|e| anyhow::anyhow!("download head manifest blob {head_hash}: {e}"))?;

    // Walk the chain target_link → ancestor (or genesis), collecting
    // each (manifest, link) pair.
    let mut manifests: Vec<(Manifest, Link)> = Vec::new();
    let stop = ancestor.as_ref();
    let mut current_link = target_link.clone();
    loop {
        download_hash(current_link.hash(), &[peer_id], blobs, endpoint).await?;
        let manifest: Manifest = blobs.get_cbor(&current_link).await?;

        if stop.is_some_and(|sl| sl == &current_link) {
            break;
        }

        manifests.push((manifest.clone(), current_link.clone()));

        if *manifest.previous() == Link::default() {
            break;
        }
        current_link = manifest.previous().clone();
    }
    manifests.reverse();

    // Self-certification: if the walk reached a genesis, its hash
    // must equal the claimed vault id (see `zim_core::vault::VaultId`).
    if let Some((first_manifest, first_link)) = manifests.first() {
        if *first_manifest.previous() == Link::default() {
            let derived = VaultId::from_genesis_link(first_link);
            if derived != vault_id {
                anyhow::bail!(
                    "chain genesis hashes to {derived}, not the claimed vault id {vault_id} — \
                     chain rejected"
                );
            }
        }
    }

    // Append every entry and download every pin. Pin downloads use
    // the manifest's shareholders as discovery hints (same logic as
    // `Vault::download_pins`).
    for (manifest, link) in &manifests {
        let previous = {
            let p = manifest.previous().clone();
            if p == Link::default() {
                None
            } else {
                Some(p)
            }
        };
        log.append(
            vault_id,
            manifest.name().to_string(),
            link.clone(),
            previous,
            manifest.height(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("log.append height {}: {e}", manifest.height()))?;

        let pin_peers: Vec<PublicKey> = manifest
            .shares()
            .iter()
            .filter_map(|(_, share)| share.identity().pubkey().copied())
            .collect();
        if pin_peers.is_empty() {
            continue;
        }
        for hash in manifest.pins().iter() {
            download_hash(*hash, &pin_peers, blobs, endpoint).await?;
        }
    }

    Ok(())
}

async fn download_hash(
    hash: Hash,
    peer_ids: &[PublicKey],
    blobs: &BlobsProvider,
    endpoint: &Endpoint,
) -> anyhow::Result<()> {
    if blobs.stat(&hash).await.unwrap_or(false) {
        return Ok(());
    }
    let downloader = Downloader::new(blobs.protocol().store(), endpoint);
    let discovery = Shuffled::new(
        peer_ids
            .iter()
            .map(zim_core::iroh::to_iroh_public_key)
            .collect(),
    );
    downloader.download(hash, discovery).await?;
    Ok(())
}
