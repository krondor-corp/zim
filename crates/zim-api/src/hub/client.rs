//! Daemon-side hub client: a [`Client`] whose bearer is a JWT minted
//! from the daemon's identity key (`aud` = the hub URL). The browser
//! uses a plain cookie-authed [`Client`] against the same request types
//! instead of this.

use reqwest::Url;
use zim_crypto::PrivateKey;

use crate::{ApiError, Client};

use super::devices::{Device, DevicesRequest};
use super::did_doc::{DidDoc, DidDocRequest};
use super::jwt;

pub struct HubClient {
    client: Client,
    self_pubkey_hex: String,
    hub_url: String,
}

impl HubClient {
    pub fn new(hub_url: &str, secret: PrivateKey) -> Result<Self, ApiError> {
        let base = Url::parse(hub_url)?;
        let hub_url = hub_url.trim_end_matches('/').to_string();
        // Mint once at construction — JWTs are short-lived but a CLI op
        // completes well inside the TTL.
        let token = jwt::mint(&secret, &hub_url);
        Ok(Self {
            client: Client::with_bearer(&base, &token)?,
            self_pubkey_hex: secret.public().to_hex(),
            hub_url,
        })
    }

    pub fn hub_url(&self) -> &str {
        &self.hub_url
    }

    /// This identity's pubkey hex.
    pub fn self_pubkey_hex(&self) -> String {
        self.self_pubkey_hex.clone()
    }

    /// `GET /api/v0/devices` — the account's device roster.
    pub async fn devices(&self) -> Result<Vec<Device>, ApiError> {
        Ok(self.client.call(DevicesRequest).await?.devices)
    }

    /// Fetch a DID document by absolute path (public, no auth).
    pub async fn did_doc(&self, path: &str) -> Result<DidDoc, ApiError> {
        self.client
            .call(DidDocRequest {
                path: path.to_string(),
            })
            .await
    }
}
