//! `POST /api/v0/auth/device-code/{code}/approve` — bind a device-code
//! grant to the signed-in user.

use reqwest::{Client as HttpClient, RequestBuilder, Url};

use crate::ApiRequest;

/// After approval, the daemon's next signed poll enrolls itself.
/// Replies 204, so the response is `()`. **Hub route to mirror:**
/// `POST /api/v0/auth/device-code/:code/approve` (RequireUser).
pub struct GrantApproveRequest {
    pub code: String,
}

impl ApiRequest for GrantApproveRequest {
    type Response = ();
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.post(
            base.join(&format!("/api/v0/auth/device-code/{}/approve", self.code))
                .expect("code is url-safe"),
        )
    }
}
