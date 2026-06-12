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
mod error;

pub use client::ApiClient;
pub use error::ApiError;

use reqwest::{Client, RequestBuilder, Url};
use serde::de::DeserializeOwned;

pub trait ApiRequest {
    type Response: DeserializeOwned;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder;
}
