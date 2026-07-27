//! `GET /u/{user_id}/did.json` — a user's `did:web` document.
//!
//! Per the did:web resolution convention, `did:web:<host>:u:<user_id>`
//! resolves to `https://<host>/u/<user_id>/did.json`. The document lists
//! **one key per enrolled device** (web key + daemons), straight from
//! `user_peers` — so resolving a user's DID yields the full, current set of
//! pubkeys that can act as them. This is the foundation for identity-based
//! vault sharing: seal a vault secret to every key the DID resolves to, and
//! new devices gain access as soon as they're enrolled.
//!
//! The payload is `zim_did::DidDocument` — deliberately minimal, not a
//! W3C DID Core conformance claim (see that type's docs): the document is
//! a key roster, nothing more, because the host serving it is trusted for
//! the roster anyway.
//!
//! Unauthenticated by design: a DID document is public (pubkeys, never
//! secrets), exactly like the hub's own `/.well-known/did.json`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use uuid::Uuid;
use zim_crypto::PublicKey;
// Emit the same document type the daemons/SDK parse — one definition.
use zim_did::{DidDocument, VerificationMethod};

use crate::database::models::UserPeer;
use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/:user_id/did.json", get(did_doc))
        .with_state(state)
}

/// `did:key:z6Mk…` multibase tail for an ed25519 pubkey — rebuilt through
/// `zim_did` so the encoding stays in lockstep with the rest of the workspace.
fn key_multibase(pk: &PublicKey) -> String {
    let key_did = zim_did::Did::from_key(pk).to_string();
    key_did
        .strip_prefix("did:key:")
        .unwrap_or(&key_did)
        .to_string()
}

async fn did_doc(State(state): State<AppState>, Path(user_id): Path<String>) -> Response {
    let uuid = match Uuid::parse_str(user_id.trim()) {
        Ok(u) => u,
        Err(_) => return (StatusCode::NOT_FOUND, "no such identity").into_response(),
    };

    let peers = match UserPeer::list_for_user(uuid, &state.db).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("user did doc: list peers: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "lookup failed").into_response();
        }
    };
    if peers.is_empty() {
        // No keys → no resolvable identity. 404 (not an empty doc) so callers
        // can't tell an empty account from a non-existent one.
        return (StatusCode::NOT_FOUND, "no such identity").into_response();
    }

    // did:web:<host>:u:<user_id>. `state.did` is `did:web:<host>`.
    let user_did = format!("{}:u:{}", state.did, uuid);

    let verification_method: Vec<VerificationMethod> = peers
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let pk = p.peer_pubkey()?;
            Some(VerificationMethod {
                // `#key-N` is just a per-entry label (nothing references it).
                id: format!("{user_did}#key-{i}"),
                public_key_multibase: key_multibase(&pk),
            })
        })
        .collect();

    Json(DidDocument {
        id: user_did,
        verification_method,
    })
    .into_response()
}
