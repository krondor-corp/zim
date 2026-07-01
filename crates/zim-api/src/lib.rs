//! Shared HTTP API surface for zim.
//!
//! ## The pattern
//!
//! Every endpoint's **request type** implements [`ApiRequest`]: it owns
//! its own HTTP method, path, body, and auth by building a
//! [`reqwest::RequestBuilder`]. The [`Client`] is generic over the trait
//! — `client.call(SomeRequest { .. }).await?` sends it and decodes the
//! typed [`ApiRequest::Response`].
//!
//! ```ignore
//! impl ApiRequest for InitRequest {
//!     type Response = InitResponse;
//!     fn build_request(self, base: &Url, http: &Client) -> RequestBuilder {
//!         http.post(base.join("/api/v0/vault/init").unwrap()).json(&self)
//!     }
//! }
//! ```
//!
//! There is exactly one definition of this trait. The daemon's RPC
//! request types (in the `zim` crate) implement it; the [`hub`] client's
//! request types (in this crate) implement it. Same `Client`, same
//! `call`.
//!
//! ## Who depends on what
//!
//! - **`zim`** (daemon + CLI) re-exports [`ApiRequest`] / [`ApiError`] so
//!   its existing endpoint impls keep their import path, and uses the
//!   [`hub`] client for `zim hub` commands (behind `zim`'s `hub` feature).
//! - **`zim-hub`** (server) does NOT link this crate's requests at
//!   compile time — it mirrors them in axum routes by hand. Each hub
//!   request documents the route it targets; keep them in lockstep.
//! - **`zim-hub/web`** (wasm SPA) uses the [`hub`] client via reqwest's
//!   wasm/fetch backend.

#[cfg(feature = "client")]
use reqwest::{Client as HttpClient, RequestBuilder, Url};
#[cfg(feature = "client")]
use serde::de::DeserializeOwned;

#[cfg(feature = "client")]
mod error;

#[cfg(feature = "client")]
pub use error::ApiError;

#[cfg(feature = "hub")]
pub mod hub;

/// A typed HTTP request. The implementor owns everything about how the
/// request is formed — method, path, query, body, and auth headers — by
/// returning a fully-built [`RequestBuilder`]. The [`Client`] only sends
/// it and decodes [`Self::Response`].
#[cfg(feature = "client")]
pub trait ApiRequest {
    type Response: DeserializeOwned;
    fn build_request(self, base_url: &Url, client: &HttpClient) -> RequestBuilder;
}

/// Generic typed client: holds a base URL + a reqwest client and
/// executes any [`ApiRequest`].
///
/// Daemon-specific conveniences (resolve a vault/peer by name) live on
/// the `zim` crate's wrapper, not here — this stays a thin executor that
/// both the daemon RPC and the hub client share.
#[cfg(feature = "client")]
#[derive(Debug, Clone)]
pub struct Client {
    base: Url,
    http: HttpClient,
}

#[cfg(feature = "client")]
impl Client {
    pub fn new(base: &Url) -> Result<Self, ApiError> {
        Ok(Self {
            base: base.clone(),
            http: HttpClient::builder().build()?,
        })
    }

    /// Build over an existing reqwest client (lets callers share a
    /// connection pool / configured client).
    pub fn with_http(base: &Url, http: HttpClient) -> Self {
        Self {
            base: base.clone(),
            http,
        }
    }

    /// Build a client that attaches `Authorization: Bearer <token>` to
    /// every request. Auth is a property of the *client*, not the
    /// request — so the same [`ApiRequest`] types work for a daemon
    /// (bearer JWT, here) and a browser (session cookie, attached
    /// automatically by `fetch` for same-origin calls).
    pub fn with_bearer(base: &Url, token: &str) -> Result<Self, ApiError> {
        use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
        let mut headers = HeaderMap::new();
        let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|e| ApiError::Other(e.to_string()))?;
        value.set_sensitive(true);
        headers.insert(AUTHORIZATION, value);
        let http = HttpClient::builder().default_headers(headers).build()?;
        Ok(Self {
            base: base.clone(),
            http,
        })
    }

    pub fn base(&self) -> &Url {
        &self.base
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    /// Send `req` (method/path/body/auth per its own `build_request`)
    /// and decode the typed reply. Non-2xx → [`ApiError::HttpStatus`].
    pub async fn call<R: ApiRequest>(&self, req: R) -> Result<R::Response, ApiError> {
        let response = req.build_request(&self.base, &self.http).send().await?;
        if response.status().is_success() {
            Ok(response.json::<R::Response>().await?)
        } else {
            Err(ApiError::HttpStatus(
                response.status(),
                response.text().await.unwrap_or_default(),
            ))
        }
    }
}
