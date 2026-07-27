//! `/api/v0/peers/addr` + `/api/v0/peers/introduce` — direct NodeAddr
//! exchange, bypassing pkarr/relay discovery.
//!
//! `addr` reports this daemon's current iroh `NodeAddr` (node id +
//! direct socket addresses); `introduce` injects another peer's so we
//! can dial them immediately. This is the deterministic local path the
//! dev harness uses to make e2e runs hermetic — two daemons on one
//! machine shouldn't need a DHT round-trip to find each other.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};

use crate::daemon::api::client::ApiRequest;
use crate::ServiceState;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AddrRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddrResponse {
    /// This daemon's pubkey, hex (== iroh NodeId).
    pub node_id: String,
    /// Direct socket addresses the endpoint is reachable on.
    pub direct_addresses: Vec<String>,
}

pub async fn addr_handler(State(state): State<ServiceState>) -> impl IntoResponse {
    let addr = state.peer().node_addr();
    Json(AddrResponse {
        node_id: addr.node_id.to_string(),
        direct_addresses: addr.direct_addresses().map(|a| a.to_string()).collect(),
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntroduceRequest {
    /// The peer's pubkey, hex (== iroh NodeId).
    pub node_id: String,
    /// Socket addresses to dial the peer on directly.
    pub direct_addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntroduceResponse {
    pub node_id: String,
}

pub async fn introduce_handler(
    State(state): State<ServiceState>,
    Json(req): Json<IntroduceRequest>,
) -> Result<impl IntoResponse, IntroduceError> {
    let node_id: zim_peer::iroh::NodeId = req
        .node_id
        .parse()
        .map_err(|_| IntroduceError::BadNodeId(req.node_id.clone()))?;
    let addrs: Vec<std::net::SocketAddr> = req
        .direct_addresses
        .iter()
        .map(|a| a.parse())
        .collect::<Result<_, _>>()
        .map_err(|e| IntroduceError::BadAddr(format!("{e}")))?;

    let addr = zim_peer::iroh::NodeAddr::from_parts(node_id, None, addrs);
    state
        .peer()
        .introduce(addr)
        .map_err(|e| IntroduceError::Introduce(e.to_string()))?;

    Ok(Json(IntroduceResponse {
        node_id: req.node_id,
    }))
}

#[derive(Debug, thiserror::Error)]
pub enum IntroduceError {
    #[error("invalid node id: {0}")]
    BadNodeId(String),
    #[error("invalid socket address: {0}")]
    BadAddr(String),
    #[error("introduce failed: {0}")]
    Introduce(String),
}

impl IntoResponse for IntroduceError {
    fn into_response(self) -> axum::response::Response {
        let status = match self {
            IntroduceError::BadNodeId(_) | IntroduceError::BadAddr(_) => {
                axum::http::StatusCode::BAD_REQUEST
            }
            IntroduceError::Introduce(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, self.to_string()).into_response()
    }
}

impl ApiRequest for AddrRequest {
    type Response = AddrResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        client
            .post(base_url.join("/api/v0/peers/addr").unwrap())
            .json(&self)
    }
}

impl ApiRequest for IntroduceRequest {
    type Response = IntroduceResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        client
            .post(base_url.join("/api/v0/peers/introduce").unwrap())
            .json(&self)
    }
}
