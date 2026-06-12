//! `/api/v0/escrow` — passphrase-wrapped browser identity storage.
//!
//! Every verb is gated on the requesting user owning the
//! `did_fragment`. Convention: every did fragment the hub hosts has
//! the shape `did:web:<host>:u:<user_uuid>#<device_label>`; the
//! gate checks the `u:` segment against the signed-in user (or
//! admin). See [`crate::access::can_access_escrow_did`].
//!
//! Writes are first-write-wins. Phase 3 will add a signed-challenge
//! check on writes/deletes so only someone currently holding the
//! unwrapped key can rotate or revoke.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::access::can_access_escrow_did;
use crate::database::models::EscrowedKey;
use crate::http::auth::RequireUser;
use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(fetch).put(put_).delete(delete_))
        .route("/list", get(list))
        .with_state(state)
}

#[derive(Debug, Serialize)]
pub struct EscrowListItem {
    pub did: String,
    pub created_at: String,
}

/// Fragments escrowed by the signed-in user. The recovery path on a
/// fresh browser: list → pick → `GET /?did=…` → unwrap locally.
/// Only fragments + timestamps — never the wrapped blobs in bulk.
async fn list(State(state): State<AppState>, RequireUser(user): RequireUser) -> Response {
    match EscrowedKey::list_for_user(user.id(), &state.db).await {
        Ok(rows) => Json(
            rows.iter()
                .map(|r| EscrowListItem {
                    did: r.did_fragment().to_string(),
                    created_at: r.created_at().to_string(),
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => {
            tracing::warn!("escrow list for {}: {e}", user.email());
            (StatusCode::INTERNAL_SERVER_ERROR, "escrow list failed").into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct EscrowQuery {
    /// Fully-qualified DID with verification-method fragment.
    pub did: String,
}

/// Wire shape for both `PUT` requests and `GET` responses. Binary
/// fields are base64-encoded so the payload stays JSON.
#[derive(Debug, Serialize, Deserialize)]
pub struct EscrowBlob {
    pub did: String,
    pub salt: String,
    pub kdf: String,
    pub wrapped_secret: String,
    pub created_at: String,
}

async fn fetch(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
    Query(q): Query<EscrowQuery>,
) -> Response {
    if !can_access_escrow_did(&user, &q.did) {
        // 404 (not 403) to avoid confirming the row exists.
        return (StatusCode::NOT_FOUND, "no escrow").into_response();
    }
    match EscrowedKey::find(&q.did, &state.db).await {
        Ok(Some(row)) => Json(EscrowBlob {
            did: row.did_fragment().to_string(),
            salt: B64.encode(row.salt()),
            kdf: row.kdf().to_string(),
            wrapped_secret: B64.encode(row.wrapped_secret()),
            created_at: row.created_at().to_string(),
        })
        .into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "no escrow").into_response(),
        Err(e) => {
            tracing::warn!("escrow find {}: {e}", q.did);
            (StatusCode::INTERNAL_SERVER_ERROR, "escrow lookup failed").into_response()
        }
    }
}

async fn put_(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
    Json(body): Json<EscrowBlob>,
) -> Response {
    if !can_access_escrow_did(&user, &body.did) {
        return (StatusCode::FORBIDDEN, "not your did").into_response();
    }
    let salt = match B64.decode(body.salt.as_bytes()) {
        Ok(s) => s,
        Err(_) => return (StatusCode::BAD_REQUEST, "salt is not valid base64").into_response(),
    };
    let wrapped_secret = match B64.decode(body.wrapped_secret.as_bytes()) {
        Ok(s) => s,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                "wrapped_secret is not valid base64",
            )
                .into_response()
        }
    };
    match EscrowedKey::create(&body.did, &salt, &body.kdf, &wrapped_secret, &state.db).await {
        Ok(_) => StatusCode::CREATED.into_response(),
        Err(sqlx::Error::Database(e)) if is_unique_violation(&*e) => {
            (StatusCode::CONFLICT, "already escrowed").into_response()
        }
        Err(e) => {
            tracing::warn!("escrow put {}: {e}", body.did);
            (StatusCode::INTERNAL_SERVER_ERROR, "escrow put failed").into_response()
        }
    }
}

async fn delete_(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
    Query(q): Query<EscrowQuery>,
) -> Response {
    if !can_access_escrow_did(&user, &q.did) {
        return (StatusCode::NOT_FOUND, "no escrow").into_response();
    }
    match EscrowedKey::delete(&q.did, &state.db).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "no escrow").into_response(),
        Err(e) => {
            tracing::warn!("escrow delete {}: {e}", q.did);
            (StatusCode::INTERNAL_SERVER_ERROR, "escrow delete failed").into_response()
        }
    }
}

fn is_unique_violation(e: &dyn sqlx::error::DatabaseError) -> bool {
    // sqlite + libsqlite3-sys exposes SQLITE_CONSTRAINT_PRIMARYKEY
    // (code 1555) for PK violations. Map it generically so future
    // back-ends don't have to special-case.
    e.message().contains("UNIQUE constraint failed")
}
