//! `/api/v0/blob` — ciphertext blob store.
//!
//! Only the write side is typed here: `GET /api/v0/blob/{hash}` replies
//! with the **raw ciphertext body** (no JSON), which doesn't fit
//! [`ApiRequest`]'s typed-JSON decode — callers that need it (the wasm
//! SDK) fetch it through their own byte-level dispatch against the same
//! route.

use reqwest::{Client as HttpClient, RequestBuilder, Url};
use serde::{Deserialize, Serialize};

use crate::ApiRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteBlobResponse {
    /// blake3 hex of the stored blob.
    pub hash: String,
}

/// `PUT /api/v0/blob` — store ciphertext, returns its hash.
/// **Hub route to mirror:** `PUT /api/v0/blob` (RequireUser).
pub struct PutBlobRequest {
    pub data: Vec<u8>,
}

impl ApiRequest for PutBlobRequest {
    type Response = WriteBlobResponse;
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.put(base.join("/api/v0/blob").expect("static path"))
            .body(self.data)
    }
}
