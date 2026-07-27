//! `GET /.well-known/did.json` — the hub's `did:web` document.
//!
//! Per the did:web resolution convention, the document for
//! `did:web:<host>` lives at `https://<host>/.well-known/did.json`.
//! Resolving the hub's DID is then a single unauthenticated GET; any
//! peer can derive the hub's current iroh pubkey (the network identity
//! it dials on) without out-of-band coordination.
//!
//! The payload is `zim_did::DidDocument` — deliberately minimal, not a
//! W3C DID Core conformance claim (see that type's docs). The doc is
//! unsigned by construction: `did:web` trust *is* host trust, so key
//! rotation means a fresh hub identity and a new `did:web`.

use axum::extract::State;
use axum::routing::get;
use axum::Json;
use axum::Router;
// Emit the same document type the daemons/SDK parse — one definition.
use zim_did::{DidDocument, VerificationMethod};

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/did.json", get(did_doc))
        .with_state(state)
}

async fn did_doc(State(state): State<AppState>) -> Json<DidDocument> {
    let pk = state.peer.secret().public();
    // `did:key:z6Mk…` for the hub's own iroh identity. The pubkey is
    // the multibase tail. We rebuild it through `zim_did` so the
    // encoding stays in lockstep with the rest of the workspace.
    let key_did = zim_did::Did::from_key(&pk);
    let key_did_str = key_did.to_string();
    let multibase = key_did_str
        .strip_prefix("did:key:")
        .unwrap_or(&key_did_str)
        .to_string();

    Json(DidDocument {
        id: state.did.clone(),
        verification_method: vec![VerificationMethod {
            // `#key-0` is just a per-entry label (nothing references it).
            id: format!("{}#key-0", state.did),
            public_key_multibase: multibase,
        }],
    })
}
