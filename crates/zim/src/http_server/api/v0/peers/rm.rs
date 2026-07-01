use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};

use crate::http_server::api::client::ApiRequest;
use crate::ServiceState;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RmRequest {
    pub nick: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RmResponse {
    pub nick: String,
    /// DID URL of the removed peer.
    pub did: String,
}

pub async fn handler(
    State(state): State<ServiceState>,
    Json(req): Json<RmRequest>,
) -> Result<impl IntoResponse, RmError> {
    use zim_peer::{PeerStore, PeerStoreError};
    let removed = match state.peers().remove(&req.nick).await {
        Ok(entry) => entry,
        Err(PeerStoreError::NotFound(n)) => return Err(RmError::NotFound(n)),
        Err(e) => return Err(RmError::Storage(e.to_string())),
    };
    let did = removed.identity.to_string();
    Ok((
        http::StatusCode::OK,
        Json(RmResponse {
            nick: req.nick,
            did,
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum RmError {
    #[error("unknown peer: {0}")]
    NotFound(String),
    #[error("peer book: {0}")]
    Storage(String),
}

impl IntoResponse for RmError {
    fn into_response(self) -> axum::response::Response {
        match self {
            RmError::NotFound(_) => (http::StatusCode::NOT_FOUND, self.to_string()).into_response(),
            RmError::Storage(_) => {
                (http::StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
            }
        }
    }
}

impl ApiRequest for RmRequest {
    type Response = RmResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        client
            .post(base_url.join("/api/v0/peers/rm").unwrap())
            .json(&self)
    }
}
