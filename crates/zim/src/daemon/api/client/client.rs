//! Typed HTTP client. One method: [`ApiClient::call`].

use reqwest::Client;
use url::Url;
use zim_core::vault::VaultId;

use super::ApiError;
use super::ApiRequest;
use crate::daemon::api::v0::peers::list::ListRequest as PeersListRequest;
use crate::daemon::api::v0::vault::list::ListRequest;

#[derive(Debug, Clone)]
pub struct ApiClient {
    remote: Url,
    client: Client,
}

impl ApiClient {
    pub fn new(remote: &Url) -> Result<Self, ApiError> {
        let client = Client::builder().build()?;
        Ok(Self {
            remote: remote.clone(),
            client,
        })
    }

    pub fn remote(&self) -> &Url {
        &self.remote
    }

    pub fn http(&self) -> &Client {
        &self.client
    }

    /// POST/GET (per the request's own `build_request`) and decode
    /// the typed reply. Non-2xx → [`ApiError::HttpStatus`].
    pub async fn call<R: ApiRequest>(&self, req: R) -> Result<R::Response, ApiError> {
        let response = req.build_request(&self.remote, &self.client).send().await?;
        if response.status().is_success() {
            Ok(response.json::<R::Response>().await?)
        } else {
            Err(ApiError::HttpStatus(
                response.status(),
                response.text().await.unwrap_or_default(),
            ))
        }
    }

    /// Resolve a vault identifier — id hex or human name — to its
    /// `VaultId`. Tries the hex parse first; on failure falls back to
    /// `POST /api/v0/vault/list` and finds by exact name match.
    pub async fn resolve_vault(&self, identifier: &str) -> Result<VaultId, ApiError> {
        if let Ok(id) = identifier.parse::<VaultId>() {
            return Ok(id);
        }
        let listing = self.call(ListRequest {}).await?;
        listing
            .vaults
            .into_iter()
            .find(|v| v.name.as_deref() == Some(identifier))
            .map(|v| v.vault_id)
            .ok_or_else(|| {
                ApiError::HttpStatus(
                    reqwest::StatusCode::NOT_FOUND,
                    format!("no vault named '{identifier}'"),
                )
            })
    }

    /// Resolve a peer identifier — DID URL or nickname from the
    /// daemon's address book — to a DID URL string. Same shape as
    /// [`Self::resolve_vault`]: pass DIDs through unchanged, look
    /// up nicks. Used by every CLI op that takes a peer.
    ///
    /// As a convenience we also accept a bare 64-char hex pubkey and
    /// synthesise the corresponding `did:key:` URL. Saves users
    /// pasting raw hex (e.g. from `zim id`) without first having to
    /// add it to the peer book.
    pub async fn resolve_peer(&self, identifier: &str) -> Result<String, ApiError> {
        if identifier.starts_with("did:") {
            return Ok(identifier.to_string());
        }
        if identifier.len() == 64 && identifier.chars().all(|c| c.is_ascii_hexdigit()) {
            let pk = zim_crypto::PublicKey::from_hex(identifier).map_err(|e| {
                ApiError::HttpStatus(
                    reqwest::StatusCode::BAD_REQUEST,
                    format!("bad hex pubkey: {e}"),
                )
            })?;
            return Ok(format!("did:key:{}", zim_did::did_key_encode(&pk)));
        }
        let listing = self.call(PeersListRequest::default()).await?;
        listing
            .peers
            .into_iter()
            .find(|p| p.nick == identifier)
            .map(|p| p.did)
            .ok_or_else(|| {
                ApiError::HttpStatus(
                    reqwest::StatusCode::NOT_FOUND,
                    format!("no peer named '{identifier}'"),
                )
            })
    }
}
