use axum::extract::State;
use axum::routing::get;
use axum::Json;
use axum::Router;
use serde::Serialize;

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/livez", get(livez))
        .route("/readyz", get(readyz))
        .route("/version", get(version))
        .route("/identity", get(identity))
        .with_state(state)
}

async fn livez() -> &'static str {
    "ok"
}

async fn readyz() -> &'static str {
    "ok"
}

async fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Serialize)]
struct Identity {
    /// Canonical hub identity — `did:web:<host>`. Resolves via
    /// `GET /.well-known/did.json` on this hub. This is what users
    /// pass to `zim peers add hub <did>` on their daemon.
    did: String,
    /// Convenience: the DID document URL the daemon will fetch to
    /// resolve this DID. Same content as `did` plus a scheme — saves
    /// callers from re-deriving it.
    did_doc_url: String,
}

async fn identity(State(state): State<AppState>) -> Json<Identity> {
    Json(Identity {
        did: state.did.clone(),
        did_doc_url: format!("http://{}/.well-known/did.json", state.listen_address),
    })
}
