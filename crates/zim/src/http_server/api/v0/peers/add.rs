use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_did::Identity;

use crate::http_server::api::client::ApiRequest;
use crate::ServiceState;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddRequest {
    pub nick: String,
    /// DID URL of the peer being added.
    pub did: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddResponse {
    pub nick: String,
    pub did: String,
}

pub async fn handler(
    State(state): State<ServiceState>,
    Json(req): Json<AddRequest>,
) -> Result<impl IntoResponse, AddError> {
    // Validate the DID parses before storing — better to fail fast
    // than persist garbage that resolver-callers will rediscover
    // later. We don't keep the parsed Identity; the DID string is
    // round-tripped through TOML and re-parsed on read.
    Identity::parse(&req.did).map_err(|e| AddError::BadDid(e.to_string()))?;

    let mut book =
        crate::peers::PeerBook::load(state.home()).map_err(|e| AddError::Storage(e.to_string()))?;
    book.upsert(req.nick.clone(), req.did.clone(), req.notes);
    book.save(state.home())
        .map_err(|e| AddError::Storage(e.to_string()))?;
    Ok((
        http::StatusCode::OK,
        Json(AddResponse {
            nick: req.nick,
            did: req.did,
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum AddError {
    #[error("invalid DID: {0}")]
    BadDid(String),
    #[error("peer book: {0}")]
    Storage(String),
}

impl IntoResponse for AddError {
    fn into_response(self) -> axum::response::Response {
        match self {
            AddError::BadDid(_) => {
                (http::StatusCode::BAD_REQUEST, self.to_string()).into_response()
            }
            AddError::Storage(_) => {
                (http::StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
            }
        }
    }
}

impl ApiRequest for AddRequest {
    type Response = AddResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        client
            .post(base_url.join("/api/v0/peers/add").unwrap())
            .json(&self)
    }
}
