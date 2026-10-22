//! `GET /api/v0/auth/providers` — how this hub lets people sign in.

use serde::{Deserialize, Serialize};

use reqwest::{Client as HttpClient, RequestBuilder, Url};

use crate::ApiRequest;

/// The sign-in options a hub offers, in the order to show them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthProviders {
    pub providers: Vec<SignInOption>,
}

/// One way to sign in, described by the UI it needs rather than by
/// provider, so a new provider of an existing kind needs no SPA change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SignInOption {
    /// A button: navigate the browser to `path` (e.g. an OAuth handshake).
    Redirect {
        /// The provider's `ZIM_HUB_AUTH` name (`google`).
        id: String,
        label: String,
        path: String,
    },
    /// An email field POSTed as a form (`email=…`) to `action`.
    Email {
        /// The provider's `ZIM_HUB_AUTH` name (`dev-trust`).
        id: String,
        label: String,
        action: String,
        /// Pre-fill for the email field.
        #[serde(default)]
        default_email: Option<String>,
        /// Shown with the option — for ones an operator should think
        /// twice about.
        #[serde(default)]
        warning: Option<String>,
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
