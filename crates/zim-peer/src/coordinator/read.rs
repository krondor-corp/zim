//! The read path: file reads that fetch content from a provider on a
//! local miss, feeding the on-demand [`ContentCache`](crate::ContentCache).

use zim_core::blobs::BlobStore;
use zim_core::fs::{AbsPath, Entry, FsError, Manifest};
use zim_core::linked_data::Hash;
use zim_core::vault::{VaultId, VaultLog};
use zim_crypto::PublicKey;

use super::SyncCoordinator;
use crate::Vault;

/// Why [`SyncCoordinator::cat`] couldn't produce a file's bytes.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    /// The path didn't resolve, isn't a file, or the local read failed.
    #[error(transparent)]
    Fs(#[from] FsError),
    /// The file's content isn't local and no provider could serve it
    /// (all shareholders and the pull source offline or missing it).
    #[error("content {hash} unavailable: no provider reachable")]
    ContentUnavailable {
        hash: Hash,
        #[source]
        source: anyhow::Error,
    },
}

impl<L> SyncCoordinator<L>
where
    L: VaultLog + Clone + 'static,
    L::Error: std::error::Error + Send + Sync + 'static,
{
    /// Read the file at `path` in `vault`, fetching its content from a
    /// provider first if it isn't local. Under [`PinPolicy::Full`] a
    /// miss is a hole the sweep hasn't healed yet; under
    /// [`PinPolicy::OnDemand`] it's the normal first read, and the
    /// fetched blob joins the LRU (evicting older fetched content past
    /// the budget).
    pub async fn cat(&self, vault: &Vault<L>, path: &AbsPath) -> Result<Vec<u8>, ReadError> {
        if let Some(Entry::File { link, .. }) = vault.fs().get_entry_at_path(path).await? {
            self.ensure_content(vault.id(), vault.manifest(), link.hash())
                .await?;
        }
        Ok(vault.fs().cat(path).await?)
    }

    /// Make the content blob `hash` of `vault_id` local. Providers are
    /// `manifest`'s share reach targets plus whoever we last pulled the
    /// vault from.
    pub async fn ensure_content(
        &self,
        vault_id: VaultId,
        manifest: &Manifest,
        hash: Hash,
    ) -> Result<(), ReadError> {
        if self.blobs.stat(&hash).await.unwrap_or(false) {
            if let Some(cache) = &self.content_cache {
                cache.touch(&hash);
            }
            return Ok(());
        }

        let self_pk = self.secret.public();
        let sources = self
            .pull_sources
            .lock()
            .unwrap()
            .get(&vault_id)
            .cloned()
            .unwrap_or_default();
        let mut providers: Vec<PublicKey> = manifest
            .shares()
            .iter()
            .filter_map(|(_, share)| share.reach())
            .chain(sources)
            .filter(|pk| *pk != self_pk)
            .collect();
        providers.sort();
        providers.dedup();

        let unavailable = |source: anyhow::Error| ReadError::ContentUnavailable { hash, source };
        if providers.is_empty() {
            return Err(unavailable(anyhow::anyhow!("no providers")));
        }
        self.download_hash(hash, &providers)
            .await
            .map_err(unavailable)?;
        tracing::debug!(%vault_id, %hash, "fetched content on read");

        if let Some(cache) = &self.content_cache {
            let size = self.blobs.size(&hash).await.ok().flatten().unwrap_or(0);
            for victim in cache.admit(hash, size) {
                match self.blobs.evict(victim).await {
                    Ok(()) => tracing::debug!(hash = %victim, "content cache: evicted"),
                    Err(e) => tracing::warn!(hash = %victim, "content cache: evict: {e}"),
                }
            }
        }
        Ok(())
    }
}
