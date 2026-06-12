//! Daemon-managed filesystem backup sync (T-018).
//!
//! `BackupSyncService` implements [`zim_runtime::Service`]. On each poll
//! interval it checks every active `SyncTarget`, compares the bucket's
//! current head against `last_head`, and materializes changed files to the
//! target directory when they diverge.
//!
//! **v1 strategy**: poll-based. A future event-channel (thing2's P-6
//! `broadcast::Sender<Event>`) would let us react to commits in real time;
//! until then, the poll interval is the latency bound.

use std::path::Path;
use std::time::Duration;
use zim_core::fs::AbsPath;

use tokio::sync::watch;
use uuid::Uuid;

use zim_protocol::BucketLogProvider;

use crate::database::models::sync_target::{SyncStatus, SyncTarget};
use crate::Database;

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(30);

pub struct BackupSyncService;

#[async_trait::async_trait]
impl zim_runtime::Service for BackupSyncService {
    type State = BackupSyncState;

    async fn run(state: Self::State, mut shutdown_rx: watch::Receiver<()>) {
        tracing::info!(
            "backup sync service started (poll interval {:?})",
            DEFAULT_POLL_INTERVAL
        );

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    tracing::info!("backup sync service shutting down");
                    return;
                }
                _ = tokio::time::sleep(DEFAULT_POLL_INTERVAL) => {
                    if let Err(e) = poll_once(&state).await {
                        tracing::error!("backup sync poll error: {e}");
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct BackupSyncState {
    pub database: Database,
    pub peer: zim_protocol::Peer<Database>,
}

async fn poll_once(state: &BackupSyncState) -> anyhow::Result<()> {
    let targets = SyncTarget::list_active(&state.database)?;
    if targets.is_empty() {
        return Ok(());
    }

    for target in targets {
        let bucket_id: Uuid = target.bucket_id;
        let target_id: Uuid = target.id;

        let current_head = match state.peer.logs().head(bucket_id, None).await {
            Ok((link, _)) => link.hash().to_string(),
            Err(e) => {
                tracing::warn!("backup sync: could not get head for bucket {bucket_id}: {e}");
                continue;
            }
        };

        let needs_sync = target
            .last_head
            .as_ref()
            .map(|h| h != &current_head)
            .unwrap_or(true);

        if !needs_sync {
            continue;
        }

        tracing::info!(
            "backup sync: bucket {bucket_id} head changed → syncing to {}",
            target.target_path
        );

        match sync_to_disk(state, bucket_id, &target.target_path).await {
            Ok(()) => {
                SyncTarget::update_head(target_id, &current_head, &state.database)?;
                tracing::info!(
                    "backup sync: bucket {bucket_id} synced to {} (head {current_head})",
                    target.target_path
                );
            }
            Err(e) => {
                tracing::error!(
                    "backup sync: failed to sync bucket {bucket_id} to {}: {e}",
                    target.target_path
                );
                SyncTarget::set_status(
                    target_id,
                    SyncStatus::Error,
                    Some(&e.to_string()),
                    &state.database,
                )?;
            }
        }
    }

    Ok(())
}

/// Full tree materialization: decrypt every file in the bucket's current
/// head and write it to `target_path`. v1 is a full dump on every sync;
/// incremental tree-diff comes as a follow-up.
async fn sync_to_disk(
    state: &BackupSyncState,
    bucket_id: Uuid,
    target_path: &str,
) -> anyhow::Result<()> {
    use std::path::PathBuf;

    let mount = state.peer.mount(bucket_id).await?;
    let target = PathBuf::from(target_path);

    std::fs::create_dir_all(&target)?;

    let inner = mount.inner().await;
    let manifest = inner.manifest();

    // Write a `.zim-sync` marker file with bucket metadata.
    let marker = serde_json::json!({
        "bucket_id": bucket_id.to_string(),
        "bucket_name": manifest.name(),
        "synced_at": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    });
    std::fs::write(
        target.join(".zim-sync"),
        serde_json::to_string_pretty(&marker)?,
    )?;

    // Walk the tree and materialize files.
    materialize_tree(&mount, &target, Path::new("/")).await?;

    Ok(())
}

/// Recursively walk the filesystem tree and write decrypted files to disk.
async fn materialize_tree(
    mount: &zim_core::fs::Fs<zim_core::blobs::BlobsStore>,
    target_dir: &Path,
    fs_path: &Path,
) -> anyhow::Result<()> {
    let entries = mount.ls(&AbsPath::from_abs(fs_path.to_path_buf())).await?;

    for (name, node_link) in entries {
        let child_path = fs_path.join(&name);
        let disk_path = target_dir.join(child_path.strip_prefix("/").unwrap_or(&child_path));

        if node_link.is_file() {
            // File: decrypt and write.
            if let Some(parent) = disk_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let data = mount.cat(&AbsPath::from_abs(child_path.clone())).await?;
            std::fs::write(&disk_path, &data)?;
            tracing::debug!("materialized {}", disk_path.display());
        } else {
            // Directory: recurse.
            std::fs::create_dir_all(&disk_path)?;
            Box::pin(materialize_tree(mount, target_dir, &child_path)).await?;
        }
    }

    Ok(())
}
