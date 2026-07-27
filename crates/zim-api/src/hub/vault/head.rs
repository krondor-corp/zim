//! `GET /api/v0/vaults/{vault_id}/head` — current canonical head + height.

use reqwest::{Client as HttpClient, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_core::linked_data::Link;
use zim_core::vault::VaultId;

use crate::ApiRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadResponse {
    pub link: Link,
    pub height: u64,
}

/// **Hub route to mirror:** `GET /api/v0/vaults/:vault_id/head` (RequireUser;
/// non-owners get 404).
pub struct HeadRequest {
    pub vault_id: VaultId,
}

impl ApiRequest for HeadRequest {
    type Response = HeadResponse;
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.get(
            base.join(&format!("/api/v0/vaults/{}/head", self.vault_id))
                .expect("vault id is hex"),
        )
    }
}
