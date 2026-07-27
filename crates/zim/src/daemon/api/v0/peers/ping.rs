use std::time::Instant;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_did::Did;

use crate::daemon::api::client::ApiRequest;
use crate::ServiceState;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PingRequest {
    /// DID URL of the peer to ping. The CLI resolves nicknames to
    /// DIDs before sending.
    pub peer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingResponse {
    /// DID we sent.
    pub peer: String,
    /// What the remote claims their DID is. Should equal `peer`;
    /// a mismatch means the address-book entry is stale.
    pub peer_id_reported: String,
    pub version: String,
    pub uptime_secs: u64,
    /// Wall-clock round trip from "send" to "got reply", in ms.
    pub rtt_ms: u64,
}

pub async fn handler(
    State(state): State<ServiceState>,
    Json(req): Json<PingRequest>,
) -> Result<impl IntoResponse, PingError> {
    // The iroh transport addresses peers by pubkey; resolve the DID
    // to one. For `did:key` it's a no-op; for `did:web` the
    // resolver does an HTTPS GET on the hub's `did.json` and picks
    // the first peer-purpose verification method.
    let identity = Did::parse(&req.peer).map_err(|e| PingError::BadDid(e.to_string()))?;
    let peer = zim_did::resolve_pubkey(&identity, state.resolver().as_ref())
        .await
        .map_err(|e| PingError::BadDid(e.to_string()))?;

    // Round-trip through whatever PeerSender the coordinator was
    // built with — in production that's `IrohPeerSender`, which dials
    // the peer over the existing sync ALPN. RTT is wall-clock from
    // call to reply; it captures TCP/QUIC connect + 1 round trip.
    let start = Instant::now();
    let reply = state
        .peer()
        .coord()
        .peer_sender()
        .send_ping(peer, zim_peer::PingRequest)
        .await
        .map_err(|e| PingError::Send(e.to_string()))?;
    let rtt_ms = start.elapsed().as_millis() as u64;

    // Reply's `peer_id` is a hex string today; re-wrap as a DID URL
    // so the client compares DID-to-DID.
    let reported_pk = zim_crypto::PublicKey::from_hex(&reply.peer_id)
        .map_err(|e| PingError::Send(format!("remote sent malformed pubkey hex: {e}")))?;
    let peer_id_reported = format!("did:key:{}", zim_did::did_key_encode(&reported_pk));

    Ok((
        http::StatusCode::OK,
        Json(PingResponse {
            peer: req.peer,
            peer_id_reported,
            version: reply.version,
            uptime_secs: reply.uptime_secs,
            rtt_ms,
        }),
    )
        .into_response())
}

#[derive(Debug, thiserror::Error)]
pub enum PingError {
    #[error("invalid DID: {0}")]
    BadDid(String),
    #[error("ping: {0}")]
    Send(String),
}

impl IntoResponse for PingError {
    fn into_response(self) -> axum::response::Response {
        match self {
            PingError::BadDid(_) => {
                (http::StatusCode::BAD_REQUEST, self.to_string()).into_response()
            }
            // 502: "we tried, the remote didn't answer cleanly."
            PingError::Send(_) => (http::StatusCode::BAD_GATEWAY, self.to_string()).into_response(),
        }
    }
}

impl ApiRequest for PingRequest {
    type Response = PingResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        client
            .post(base_url.join("/api/v0/peers/ping").unwrap())
            .json(&self)
    }
}
