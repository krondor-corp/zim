//! `POST /api/v0/admin/users/{user_id}/{action}` — flip a user's
//! authorization/role.

use reqwest::{Client as HttpClient, RequestBuilder, Url};

use crate::ApiRequest;

/// `action` is one of `authorize` / `unauthorize` / `promote` /
/// `demote`; the server rejects anything else. Replies with no body, so
/// the response is `()`. **Hub route to mirror:**
/// `POST /api/v0/admin/users/:user_id/:action` (RequireAdmin).
pub struct AdminActionRequest {
    pub user_id: String,
    pub action: String,
}

impl ApiRequest for AdminActionRequest {
    type Response = ();
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.post(
            base.join(&format!(
                "/api/v0/admin/users/{}/{}",
                self.user_id, self.action
            ))
            .expect("uuid + action are path-safe"),
        )
    }
}
