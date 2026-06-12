//! `GET /.well-known/did.json` — the hub's `did:web` document.
//!
//! Per the did:web method spec, the document for `did:web:<host>` lives
//! at `https://<host>/.well-known/did.json`. Resolving the hub's DID is
//! then a single unauthenticated GET; any peer can derive the hub's
//! current iroh pubkey (the network identity it dials on) without
//! out-of-band coordination.
//!
//! Phase 1: doc is unsigned and lists exactly one verification method —
//! the hub's iroh pubkey, marked as a `peer` purpose so dialers know
//! it's reachable. Phase 3 adds signed key rotation; today, rotation
//! means a fresh hub identity and a new `did:web` (because there's no
//! controller to sign the next doc).

use axum::extract::State;
use axum::routing::get;
use axum::Json;
use axum::Router;
use serde::Serialize;

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/did.json", get(did_doc))
        .with_state(state)
}

#[derive(Serialize)]
struct DidDocument {
    #[serde(rename = "@context")]
    context: Vec<&'static str>,
    id: String,
    #[serde(rename = "verificationMethod")]
    verification_method: Vec<VerificationMethod>,
}

#[derive(Serialize)]
struct VerificationMethod {
    id: String,
    #[serde(rename = "type")]
    method_type: &'static str,
    controller: String,
    #[serde(rename = "publicKeyMultibase")]
    public_key_multibase: String,
    /// Non-standard zim extension: distinguishes dialable iroh peers
    /// from browser-resident web keys. Replaces the old `dialable: bool`
    /// flag on `Share`.
    purpose: &'static str,
}

async fn did_doc(State(state): State<AppState>) -> Json<DidDocument> {
    let pk = state.service.peer().secret().public();
    // `did:key:z6Mk…` for the hub's own iroh identity. The pubkey is
    // the multibase tail. We rebuild it through `zim_did` so the
    // encoding stays in lockstep with the rest of the workspace.
    let key_did = zim_did::Identity::Key(pk).to_did();
    let key_did_str = key_did.to_string();
    let multibase = key_did_str
        .strip_prefix("did:key:")
        .unwrap_or(&key_did_str)
        .to_string();

    Json(DidDocument {
        context: vec![
            "https://www.w3.org/ns/did/v1",
            "https://w3id.org/security/suites/ed25519-2020/v1",
        ],
        id: state.did.clone(),
        verification_method: vec![VerificationMethod {
            id: format!("{}#peer", state.did),
            method_type: "Ed25519VerificationKey2020",
            controller: state.did.clone(),
            public_key_multibase: multibase,
            purpose: "peer",
        }],
    })
}
