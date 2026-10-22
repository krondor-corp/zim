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
    /// OAuth/OIDC: a button that sends the browser to `path`, where the
    /// hub starts the provider's handshake and mints the session on the
    /// callback. Any such provider (Google today) fits this shape.
    #[serde(rename = "oauth")]
    OAuth {
        /// The provider's `ZIM_HUB_AUTH` name (`google`).
        id: String,
        label: String,
        path: String,
    },
    /// Dev-only trust login: no verification — whatever email is POSTed
    /// (`email=…`) to `action` gets an admin session. Only offered by
    /// `--features dev-login` hubs on loopback; the SPA renders it with a
    /// warning.
    DevTrust { action: String },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_in_options_are_tagged_by_kind_on_the_wire() {
        let providers = AuthProviders {
            providers: vec![
                SignInOption::DevTrust {
                    action: "/auth/dev/login".into(),
                },
                SignInOption::OAuth {
                    id: "google".into(),
                    label: "Sign in with Google".into(),
                    path: "/auth/google/login".into(),
                },
            ],
        };

        let json = serde_json::to_value(&providers).unwrap();
        assert_eq!(json["providers"][0]["kind"], "dev-trust");
        assert_eq!(json["providers"][1]["kind"], "oauth");

        let back: AuthProviders = serde_json::from_value(json).unwrap();
        assert_eq!(back, providers);
    }
}
