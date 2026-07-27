//! `GET /api/v0/admin/users` — every account, with role/authorization.

use serde::{Deserialize, Serialize};

use reqwest::{Client as HttpClient, RequestBuilder, Url};

use crate::ApiRequest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdminUser {
    pub id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub is_admin: bool,
    pub is_authorized: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdminUsers {
    /// The requesting admin's own id — the SPA disables self-demote.
    pub current_admin_id: String,
    pub users: Vec<AdminUser>,
}

/// **Hub route to mirror:** `GET /api/v0/admin/users` (RequireAdmin).
pub struct AdminUsersRequest;

impl ApiRequest for AdminUsersRequest {
    type Response = AdminUsers;
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.get(base.join("/api/v0/admin/users").expect("static path"))
    }
}
