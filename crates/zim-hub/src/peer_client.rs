//! In-process wrapper around the embedded peer `ServiceState`.
//!
//! The hub does NOT talk to the peer over HTTP — it holds the `ServiceState`
//! directly and calls the same `Database`/`Peer`/`Fs` methods that `zim-peer`'s
//! own HTTP handlers would. This module is the boundary: handlers see a small
//! read-only API (`list_buckets`, `ls`, `cat`, `history`); internally it routes
//! to the embedded peer.

use std::path::{Path, PathBuf};

use time::OffsetDateTime;
use uuid::Uuid;

use zim_fs::{FsError, NodeLink};
use zim_peer::ServiceState;

#[derive(Debug, thiserror::Error)]
pub enum PeerError {
    #[error("database error: {0}")]
    Database(String),
    #[error("fs error: {0}")]
    Fs(#[from] FsError),
}

#[derive(Clone)]
pub struct PeerClient {
    service: ServiceState,
}

impl PeerClient {
    pub fn new(service: ServiceState) -> Self {
        Self { service }
    }

    pub async fn list_buckets(&self) -> Result<Vec<BucketInfo>, PeerError> {
        let rows = self
            .service
            .database()
            .list_buckets(None, None)
            .await
            .map_err(|e| PeerError::Database(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| BucketInfo {
                bucket_id: r.id,
                name: r.name,
                link_hash: r.link.to_string(),
                created_at: r.created_at,
            })
            .collect())
    }

    pub async fn ls(&self, bucket_id: Uuid, path: &str) -> Result<Vec<PathInfo>, PeerError> {
        let fs = self.service.peer().mount_for_read(bucket_id).await?;
        let entries = fs.ls(Path::new(path)).await?;
        let mut out = Vec::with_capacity(entries.len());
        for (entry_path, link) in entries {
            out.push(PathInfo::from_entry(entry_path, &link));
        }
        Ok(out)
    }

    pub async fn cat(&self, bucket_id: Uuid, path: &str) -> Result<CatResult, PeerError> {
        let fs = self.service.peer().mount_for_read(bucket_id).await?;
        let bytes = fs.cat(Path::new(path)).await?;
        let mime_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();
        Ok(CatResult {
            path: path.to_string(),
            size: bytes.len(),
            mime_type,
            bytes,
        })
    }

    pub async fn history(
        &self,
        bucket_id: Uuid,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<HistoryEntry>, PeerError> {
        let logs = self
            .service
            .database()
            .get_bucket_logs(&bucket_id, page, page_size)
            .await
            .map_err(|e| PeerError::Database(e.to_string()))?;
        Ok(logs
            .into_iter()
            .map(|e| HistoryEntry {
                link_hash: e.current_link.to_string(),
                height: e.height,
                published: e.published,
                created_at: e.created_at,
            })
            .collect())
    }
}

// Public types consumed by handlers / templates. Kept simple and template-friendly.

#[derive(Debug, Clone)]
pub struct BucketInfo {
    pub bucket_id: Uuid,
    pub name: String,
    pub link_hash: String,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct PathInfo {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub mime_type: String,
}

impl PathInfo {
    fn from_entry(entry_path: PathBuf, link: &NodeLink) -> Self {
        let path = entry_path.to_string_lossy().into_owned();
        let name = entry_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        let is_dir = matches!(link, NodeLink::Dir(..));
        let mime_type = match link {
            NodeLink::Dir(..) => String::from("inode/directory"),
            NodeLink::Data(_, _, data) => data.mime().map(|m| m.to_string()).unwrap_or_else(|| {
                mime_guess::from_path(&entry_path)
                    .first_or_octet_stream()
                    .to_string()
            }),
        };
        Self {
            path,
            name,
            is_dir,
            mime_type,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CatResult {
    pub path: String,
    pub bytes: Vec<u8>,
    pub size: usize,
    pub mime_type: String,
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub link_hash: String,
    pub height: u64,
    pub published: bool,
    pub created_at: OffsetDateTime,
}
