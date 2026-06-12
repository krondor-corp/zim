//! Manifest-chain primitives.
//!
//! Chains are a vault concept — the version history a vault accumulates
//! as it saves new manifests. The fs layer only knows about a single
//! manifest version; walking back along [`Manifest::previous`] links is
//! the vault layer's job.
//!
//! This module exposes the two primitives that walk those chains
//! without involving a log:
//!
//! - [`collect_ops_since`] — fetch + decrypt every version's [`OpsLog`]
//!   from a head down to a supplied ancestor (or genesis).
//! - [`merge`] — given a pre-computed ancestor, run the conflict
//!   resolver + replay onto a target [`Fs`].
//!
//! **Ancestor discovery is the caller's job.** In the vault flow the
//! caller computes it via [`VaultLog::probe`](super::log::VaultLog::probe).
//! In tests the caller knows it directly. There's no general
//! blob-walking ancestor finder here — it would always be slower than
//! the log query.

use zim_core::blobs::BlobStore;
use zim_core::fs::{ConflictFile, Fs, FsError, Manifest, MergeResult, OpsLog};
use zim_core::linked_data::{BlockEncoded, Link};
use zim_crypto::PrivateKey;

/// Walk a manifest chain from `start_link` back toward `ancestor_link`
/// (exclusive), decrypting each version's [`OpsLog`] with `secret_key`.
/// Returns the union of all collected logs.
///
/// `initial_ops` is prepended (used to include the head Fs's *unsaved*
/// pending ops). Walks stop on three conditions: reaching `ancestor_link`,
/// reaching genesis, or hitting a version where `secret_key` isn't a
/// share-holder (further history is unreadable so it's pointless to
/// keep walking).
pub async fn collect_ops_since<B: BlobStore>(
    start_link: &Link,
    ancestor_link: Option<&Link>,
    blobs: &B,
    secret_key: &PrivateKey,
    initial_ops: OpsLog,
) -> Result<OpsLog, FsError> {
    let mut all_logs: Vec<OpsLog> = Vec::new();
    if !initial_ops.is_empty() {
        all_logs.push(initial_ops);
    }

    let mut link = start_link.clone();
    loop {
        if let Some(ancestor) = ancestor_link {
            if &link == ancestor {
                break;
            }
        }

        let manifest = blobs.get_cbor::<Manifest, _>(&link).await?;

        let Some(share) = manifest.get_share(&secret_key.public()) else {
            tracing::debug!(
                "collect_ops_since: stopping at link {} - no share for current user",
                link.hash()
            );
            break;
        };
        let secret = share.secret_share().recover(secret_key)?;

        if *manifest.ops() != Link::default() {
            let encrypted = blobs.get(&manifest.ops().hash()).await?;
            let mut ops_log = OpsLog::decode(&secret.decrypt(&encrypted)?)?;
            ops_log.rebuild_clock();
            all_logs.push(ops_log);
        }

        if *manifest.previous() == Link::default() {
            break;
        }
        link = manifest.previous().clone();
    }

    all_logs.reverse();
    let mut merged = OpsLog::new();
    for log in all_logs {
        merged.merge(&log);
    }
    Ok(merged)
}

/// Merge the chain ending at `incoming_link` into `target`, given a
/// pre-computed `ancestor` link.
///
/// The incoming chain is read out of `target`'s blob store — both
/// chains are content-addressed and share storage, so the caller only
/// needs to know the head link plus the common ancestor.
///
/// `ancestor`:
/// - `Some(link)` — walk from each head back to (but not through)
///   this link. Found via [`VaultLog::probe`](super::log::VaultLog::probe).
/// - `None` — walk both chains to genesis. Use this only when the
///   chains genuinely share genesis but you couldn't determine a
///   tighter bound (e.g. cloning a new vault).
///
/// Steps:
///
/// 1. Collect every op since `ancestor` from both sides, decrypting
///    each version's ops log with `target`'s private key.
/// 2. Run the default [`ConflictFile`] resolver.
/// 3. Replay the merged log against `target` via
///    [`Fs::apply_ops`](zim_core::fs::Fs::apply_ops), which also
///    folds it into `target`'s pending log.
///
/// Does NOT save. The caller decides when to call
/// [`Fs::save`](zim_core::fs::Fs::save) — for vault-level merges the
/// save needs to be paired with a log append, which is
/// [`Vault`](super::Vault)'s job.
pub async fn merge<B: BlobStore>(
    target: &Fs<B>,
    local_link: &Link,
    secret_key: &zim_crypto::PrivateKey,
    incoming_link: &Link,
    ancestor: Option<&Link>,
) -> Result<MergeResult, FsError> {
    let resolver = ConflictFile;

    let (local_pending, local_peer_id) = {
        let inner = target.inner().await;
        (inner.ops_log.clone(), inner.public_key)
    };

    let blobs = target.blobs().inner();

    let local_ops =
        collect_ops_since(local_link, ancestor, blobs, secret_key, local_pending).await?;
    let incoming_ops =
        collect_ops_since(incoming_link, ancestor, blobs, secret_key, OpsLog::new()).await?;

    let mut merged_ops = local_ops.clone();
    let merge_result = merged_ops.merge_with_resolver(&incoming_ops, &resolver, &local_peer_id);

    target.apply_ops(&merged_ops).await?;

    Ok(merge_result)
}
