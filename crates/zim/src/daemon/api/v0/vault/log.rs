//! `POST /api/v0/vaults/:vault_id/log` — paginated version chain, newest
//! first. The daemon twin of the hub's `GET .../log`: same `from`/`limit`
//! paging and `(height, link)` entries, plus each manifest's plaintext
//! author so a history view needs no second round trip. Read a listed
//! version back with `at` on `ls` / `cat`.

use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_core::blobs::BlobStore;
use zim_core::fs::Manifest;
use zim_core::linked_data::Link;
use zim_core::vault::VaultId;

use crate::daemon::api::client::ApiRequest;
use crate::daemon::api::v0::vault::extractor::VaultHandle;
use crate::daemon::state::vault_lookup_response;
use zim_peer::VaultLookupError;

/// Same cap and default as the hub's log route.
const MAX_LIMIT: u64 = 100;
const DEFAULT_LIMIT: u64 = 20;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogRequest {
    #[serde(skip)]
    pub vault_id: VaultId,
    /// Height to start from (default: the current head).
    #[serde(default)]
    pub from: Option<u64>,
    /// Max entries to return (default 20, capped at 100).
    #[serde(default)]
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub height: u64,
    pub link: Link,
    /// The device that saved this version (hex Ed25519 key); `None` when
    /// the manifest blob isn't held locally.
    pub author: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogResponse {
    pub entries: Vec<LogEntry>,
}

pub async fn handler(
    VaultHandle { vault, .. }: VaultHandle,
    Json(req): Json<LogRequest>,
) -> Result<impl IntoResponse, LogError> {
    let limit = req.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let history = vault
        .history(req.from, limit)
        .await
        .map_err(|e| LogError::Load(e.to_string()))?;

    let mut entries = Vec::with_capacity(history.len());
    for h in history {
        let author = vault
            .blobs()
            .get_cbor::<Manifest, _>(&h.link)
            .await
            .ok()
            .map(|m| m.author().to_hex());
        entries.push(LogEntry {
            height: h.height,
            link: h.link,
            author,
        });
    }
    Ok((http::StatusCode::OK, Json(LogResponse { entries })).into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum LogError {
    #[error(transparent)]
    Vault(#[from] VaultLookupError),
    #[error("load: {0}")]
    Load(String),
}

impl IntoResponse for LogError {
    fn into_response(self) -> axum::response::Response {
        match self {
            LogError::Vault(e) => vault_lookup_response(e),
            LogError::Load(_) => {
                (http::StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
            }
        }
    }
}

impl ApiRequest for LogRequest {
    type Response = LogResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let url = base_url
            .join(&format!("/api/v0/vaults/{}/log", self.vault_id))
            .unwrap();
        client.post(url).json(&self)
    }
}
