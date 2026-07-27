//! `GET /api/v0/auth/device-code/{code}` — look up a device-code grant
//! for the browser approve page.

use serde::{Deserialize, Serialize};

use reqwest::{Client as HttpClient, RequestBuilder, Url};

use crate::ApiRequest;

/// A pending device-code grant, as the approve page sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrantInfo {
    /// `"pending"` | `"approved"` | `"expired"` | `"not_found"`.
    pub status: String,
    /// The label the daemon committed at start time (e.g. hostname).
    pub label: String,
    /// The pubkey the daemon committed at start time — shown so the
    /// user verifies they're approving the machine they expect.
    pub pubkey: String,
}

/// **Hub route to mirror:** `GET /api/v0/auth/device-code/:code`
/// (RequireUser — only a signed-in human can read it).
pub struct GrantInfoRequest {
    pub code: String,
}

impl ApiRequest for GrantInfoRequest {
    type Response = GrantInfo;
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.get(
            base.join(&format!("/api/v0/auth/device-code/{}", self.code))
                .expect("code is url-safe"),
        )
    }
}
