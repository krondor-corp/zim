//! `GET <path>` — fetch a DID document by absolute path.

use serde::Deserialize;

use reqwest::{Client as HttpClient, RequestBuilder, Url};

use crate::ApiRequest;

/// A hub's or user's DID document. We read only `id` here; full
/// verification-method resolution goes through `zim-did`.
#[derive(Debug, Deserialize)]
pub struct DidDoc {
    pub id: String,
}

/// Fetch a DID document by absolute path (e.g. `/.well-known/did.json`
/// or `/u/<id>/did.json`). Public; no auth. **Hub routes to mirror:**
/// `GET /.well-known/did.json`, `GET /u/{user_id}/did.json`.
pub struct DidDocRequest {
    pub path: String,
}

impl ApiRequest for DidDocRequest {
    type Response = DidDoc;
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.get(base.join(&self.path).expect("caller passes a valid path"))
    }
}
