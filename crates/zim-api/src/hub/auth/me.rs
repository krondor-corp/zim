//! `GET /api/v0/me` — who's signed in.

use serde::{Deserialize, Serialize};

use reqwest::{Client as HttpClient, RequestBuilder, Url};

use crate::ApiRequest;

/// The signed-in user, as `GET /api/v0/me` reports it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Me {
    pub user_id: String,
    pub host: String,
    pub email: String,
    /// The account's `did:web:<host>:u:<user_id>` — the identity the
    /// web/browser key *is* (reached via the hub, not by dialing the
    /// browser). Resolves to `/u/<user_id>/did.json`. `#[serde(default)]`
    /// tolerates an older hub that doesn't send it.
    #[serde(default)]
    pub did: String,
    /// Whether this account has a browser/web key enrolled. The SPA's
    /// web-key gate uses this to choose *create* (onboard) vs *unlock*,
    /// and to ignore a stale tab-cached seed after a hub reset.
    #[serde(default)]
    pub has_web_key: bool,
}

/// **Hub route to mirror:** `GET /api/v0/me` (RequireUser,
/// pre-onboarding — a keyless user can still fetch it to mint their
/// web key).
pub struct MeRequest;

impl ApiRequest for MeRequest {
    type Response = Me;
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.get(base.join("/api/v0/me").expect("static path"))
    }
}
