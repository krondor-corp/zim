//! `GET /api/v0/vaults` — the vaults shared to the account's keys.

use serde::{Deserialize, Serialize};

use reqwest::{Client as HttpClient, RequestBuilder, Url};

use crate::ApiRequest;

/// One vault the hub mirrors for this account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VaultItem {
    pub vault_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultsResponse {
    pub vaults: Vec<VaultItem>,
}

/// **Hub route to mirror:** `GET /api/v0/vaults` (RequireOnboardedUser).
pub struct VaultsRequest;

impl ApiRequest for VaultsRequest {
    type Response = VaultsResponse;
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.get(base.join("/api/v0/vaults").expect("static path"))
    }
}
