//! `DELETE /api/v0/devices/{pubkey}` — unenroll a device.

use reqwest::{Client as HttpClient, RequestBuilder, Url};

use crate::ApiRequest;

/// Replies 204, so the response is `()`. **Hub route to mirror:**
/// `DELETE /api/v0/devices/:pubkey` (RequireUser).
pub struct RemoveDeviceRequest {
    /// 64-char lowercase hex of the device's ed25519 public key.
    pub pubkey: String,
}

impl ApiRequest for RemoveDeviceRequest {
    type Response = ();
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.delete(
            base.join(&format!("/api/v0/devices/{}", self.pubkey))
                .expect("pubkey is hex"),
        )
    }
}
