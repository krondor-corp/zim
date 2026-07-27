//! `GET /api/v0/vaults/{vault_id}/log?from=&limit=` — paginated chain walk.

use reqwest::{Client as HttpClient, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_core::linked_data::Link;
use zim_core::vault::VaultId;

use crate::ApiRequest;

/// The query string — the hub server's `Query` extractor decodes this.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogQuery {
    /// Starting height (default 0).
    pub from: Option<u64>,
    /// Max entries to return (server-clamped).
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub height: u64,
    pub link: Link,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogResponse {
    pub entries: Vec<LogEntry>,
}

/// **Hub route to mirror:** `GET /api/v0/vaults/:vault_id/log` (RequireUser).
pub struct LogRequest {
    pub vault_id: VaultId,
    pub from: u64,
    pub limit: u64,
}

impl ApiRequest for LogRequest {
    type Response = LogResponse;
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.get(
            base.join(&format!(
                "/api/v0/vaults/{}/log?from={}&limit={}",
                self.vault_id, self.from, self.limit
            ))
            .expect("vault id is hex"),
        )
    }
}
