//! Read-only client against the local zim-peer HTTP API.
//!
//! zim-hub is a strict consumer of zim-peer's `POST /api/v0/bucket/*` surface.
//! Only the read methods (`list`, `ls`, `cat`, `history`) are implemented here;
//! mutation endpoints exist on the peer but are never called from the hub.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PeerClient {
    base: Url,
    http: reqwest::Client,
}

#[derive(Debug, thiserror::Error)]
pub enum PeerError {
    #[error("peer request failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("peer returned {status}: {body}")]
    Status {
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("base url join failed: {0}")]
    Url(#[from] url::ParseError),
}

impl PeerClient {
    pub fn new(base: Url) -> Self {
        Self {
            base,
            http: reqwest::Client::builder()
                .user_agent(concat!("zim-hub/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("reqwest client"),
        }
    }

    pub fn base(&self) -> &Url {
        &self.base
    }

    async fn post<Req: Serialize, Res: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        req: &Req,
    ) -> Result<Res, PeerError> {
        let url = self.base.join(path)?;
        let resp = self.http.post(url).json(req).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(PeerError::Status { status, body });
        }
        Ok(resp.json().await?)
    }

    pub async fn list_buckets(&self) -> Result<ListResponse, PeerError> {
        self.post("api/v0/bucket/list", &ListRequest::default())
            .await
    }

    pub async fn ls(
        &self,
        bucket_id: Uuid,
        path: Option<&str>,
        at: Option<&str>,
    ) -> Result<LsResponse, PeerError> {
        self.post(
            "api/v0/bucket/ls",
            &LsRequest {
                bucket_id,
                path: path.map(str::to_string),
                deep: None,
                at: at.map(str::to_string),
            },
        )
        .await
    }

    pub async fn cat(
        &self,
        bucket_id: Uuid,
        path: &str,
        at: Option<&str>,
    ) -> Result<CatResponse, PeerError> {
        self.post(
            "api/v0/bucket/cat",
            &CatRequest {
                bucket_id,
                path: path.to_string(),
                at: at.map(str::to_string),
                download: None,
            },
        )
        .await
    }

    pub async fn history(
        &self,
        bucket_id: Uuid,
        page: Option<u32>,
        page_size: Option<u32>,
    ) -> Result<HistoryResponse, PeerError> {
        self.post(
            "api/v0/bucket/history",
            &HistoryRequest {
                bucket_id,
                page,
                page_size,
            },
        )
        .await
    }
}

// Request / response shapes — mirror zim-peer's API types. We deliberately
// duplicate them here (rather than depend on zim-peer as a crate) to keep the
// hub independent of the peer's internal types and to avoid a cargo edge.

#[derive(Debug, Default, Serialize)]
pub struct ListRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListResponse {
    pub buckets: Vec<BucketInfo>,
}

#[derive(Debug, Deserialize)]
pub struct BucketInfo {
    pub bucket_id: Uuid,
    pub name: String,
    pub link: serde_json::Value,
    pub status: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Serialize)]
pub struct LsRequest {
    pub bucket_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deep: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LsResponse {
    pub items: Vec<PathInfo>,
}

#[derive(Debug, Deserialize)]
pub struct PathInfo {
    pub path: String,
    pub name: String,
    pub link: serde_json::Value,
    pub is_dir: bool,
    pub mime_type: String,
}

#[derive(Debug, Serialize)]
pub struct CatRequest {
    pub bucket_id: Uuid,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CatResponse {
    pub path: String,
    /// Base64-encoded file content.
    pub content: String,
    pub size: usize,
    pub mime_type: String,
}

#[derive(Debug, Serialize)]
pub struct HistoryRequest {
    pub bucket_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryResponse {
    pub bucket_id: Uuid,
    pub entries: Vec<HistoryEntry>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryEntry {
    pub link_hash: String,
    pub height: u64,
    pub published: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}
