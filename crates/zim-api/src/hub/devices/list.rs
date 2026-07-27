//! `GET /api/v0/devices` — the account's device roster.

use serde::{Deserialize, Serialize};

use reqwest::{Client as HttpClient, RequestBuilder, Url};

use crate::ApiRequest;

use super::Device;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicesResponse {
    pub devices: Vec<Device>,
}

/// Auth is client-level (bearer for daemons, session cookie for the
/// browser), so this request carries none. **Hub route to mirror:**
/// `GET /api/v0/devices` (RequireUser).
pub struct DevicesRequest;

impl ApiRequest for DevicesRequest {
    type Response = DevicesResponse;
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.get(base.join("/api/v0/devices").expect("static path"))
    }
}
