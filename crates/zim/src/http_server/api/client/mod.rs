//! API client.
//!
//! Every endpoint's request type implements [`ApiRequest`]:
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
//! The client is generic over the trait:
//! `client.call(InitRequest { name }).await?` returns `InitResponse`.

#[allow(clippy::module_inception)]
mod client;

pub use client::ApiClient;

// The request/response pattern lives in `zim-api` now (one definition,
// shared with the hub client). Re-exported here so the daemon's endpoint
// `impl ApiRequest` blocks and their `ApiError` imports keep this path.
pub use zim_api::{ApiError, ApiRequest};
