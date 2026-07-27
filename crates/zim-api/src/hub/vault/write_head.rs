//! `POST /api/v0/vaults/{vault_id}/head` — advance the vault head.
//!
//! The client submits the blake3 hash of a manifest blob it already
//! pushed via `PUT /api/v0/blob`; the hub verifies signature, authorship
//! and chain continuity before appending (see the hub handler's docs).

use reqwest::{Client as HttpClient, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_core::vault::VaultId;

use crate::ApiRequest;

/// The JSON body — `vault_id` rides in the path, not here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteHeadBody {
    /// blake3 hex of the new manifest blob (already uploaded via
    /// `PUT /api/v0/blob`).
    pub manifest_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteHeadResponse {
    pub hash: String,
    pub height: u64,
}

/// **Hub route to mirror:** `POST /api/v0/vaults/:vault_id/head`
/// (RequireUser).
pub struct WriteHeadRequest {
    pub vault_id: VaultId,
    pub manifest_hash: String,
}

impl ApiRequest for WriteHeadRequest {
    type Response = WriteHeadResponse;
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.post(
            base.join(&format!("/api/v0/vaults/{}/head", self.vault_id))
                .expect("vault id is hex"),
        )
        .json(&WriteHeadBody {
            manifest_hash: self.manifest_hash,
        })
    }
}
