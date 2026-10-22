//! `GET /api/v0/auth/providers` — how this hub lets people sign in.

use serde::{Deserialize, Serialize};

use reqwest::{Client as HttpClient, RequestBuilder, Url};

use crate::ApiRequest;

/// The sign-in options a hub offers, in the order to show them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthProviders {
    pub providers: Vec<SignInOption>,
}

/// One way to sign in, as the SPA's sign-in page should render it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SignInOption {
    /// A button: navigate the browser to `path` to start an OAuth-style
    /// handshake. Any such provider (Google today) fits this shape.
    Redirect {
        /// The provider's `ZIM_HUB_AUTH` name (`google`).
        id: String,
        label: String,
        path: String,
    },
    /// Dev-only trust login: no verification — whatever email is POSTed
    /// (`email=…`) to `action` gets an admin session. Only offered by
    /// `--features dev-login` hubs on loopback; the SPA renders it with a
    /// warning.
    DevTrust {
        action: String,
        /// Pre-fill for the email field.
        #[serde(default)]
        default_email: Option<String>,
    },
}

/// **Hub route to mirror:** `GET /api/v0/auth/providers` (public — an
/// anonymous visitor needs it to sign in).
pub struct AuthProvidersRequest;

impl ApiRequest for AuthProvidersRequest {
    type Response = AuthProviders;
    fn build_request(self, base: &Url, http: &HttpClient) -> RequestBuilder {
        http.get(base.join("/api/v0/auth/providers").expect("static path"))
    }
}
