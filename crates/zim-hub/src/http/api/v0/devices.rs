//! `/api/v0/devices/*` — possession-proof enrollment.
//!
//! Two endpoints:
//!
//! - `GET  /enroll-challenge` — allocate a 32-byte challenge bound
//!   to the authenticated user with a 5-minute TTL.
//! - `POST /self`             — verify an ed25519 signature of
//!   `challenge_bytes || pubkey_bytes` against the claimed pubkey,
//!   insert a `user_peers` row, consume the challenge.
//!
//! Both gated by [`RequireUser`] — daemon enrollment later uses the
//! same surface via a session token rather than a cookie.
//!
//! The paste-pubkey form that used to live at `/app/devices/add` is
//! gone: typing a hex pubkey doesn't prove possession and let
//! anyone claim metadata visibility for any pubkey.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use zim_crypto::PublicKey;

use crate::database::models::{EnrollChallenge, PeerKind, UserPeer};
use crate::http::auth::RequireUser;
use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/enroll-challenge", get(enroll_challenge))
        .route("/self", post(self_enroll))
        .route("/:pubkey", delete(remove))
        .with_state(state)
}

// Wire types are shared: `zim_api::hub::{Device, DevicesResponse}`. This
// route mirrors `zim_api::hub::DevicesRequest` (`GET /api/v0/devices`).
use zim_api::hub::{Device, DevicesResponse};

/// `GET /api/v0/devices` — the signed-in user's enrolled peers.
async fn list(State(state): State<AppState>, RequireUser(user): RequireUser) -> Response {
    match UserPeer::list_for_user(user.id(), &state.db).await {
        Ok(rows) => Json(DevicesResponse {
            devices: rows
                .iter()
                .map(|r| Device {
                    pubkey: r.peer_pubkey_hex().to_string(),
                    did: r
                        .peer_pubkey()
                        .map(|pk| zim_did::Identity::Key(pk).to_did().to_string())
                        .unwrap_or_default(),
                    label: r.label().map(str::to_string),
                    kind: r.kind().to_string(),
                    created_at: r.created_at().to_string(),
                })
                .collect(),
        })
        .into_response(),
        Err(e) => {
            tracing::error!("list devices: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "list failed").into_response()
        }
    }
}

/// `DELETE /api/v0/devices/{pubkey}` — unenroll one of the user's peers.
async fn remove(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
    Path(pubkey): Path<String>,
) -> Response {
    let pk = match PublicKey::from_hex(pubkey.trim()) {
        Ok(p) => p,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid pubkey hex").into_response(),
    };
    match UserPeer::delete_for_user(user.id(), &pk, &state.db).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "no such device").into_response(),
        Err(e) => {
            tracing::error!("delete device: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "delete failed").into_response()
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ChallengeResponse {
    /// 64-char lowercase hex. Sign `bytes_of(challenge) || bytes_of(pubkey)`
    /// with the device's ed25519 private key.
    pub challenge: String,
    pub expires_at: String,
}

async fn enroll_challenge(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
) -> Response {
    match EnrollChallenge::create(user.id(), &state.db).await {
        Ok(c) => Json(ChallengeResponse {
            challenge: c.challenge_hex().to_string(),
            expires_at: c.expires_at().to_string(),
        })
        .into_response(),
        Err(e) => {
            tracing::error!("issue enroll challenge: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "issue challenge").into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SelfEnrollBody {
    /// 64-char lowercase hex of the device's ed25519 public key.
    pub pubkey: String,
    /// Free-form user label.
    #[serde(default)]
    pub label: String,
    /// `"web"` or `"daemon"`.
    pub kind: String,
    /// The hex challenge from `GET /enroll-challenge`.
    pub challenge: String,
    /// 128-char lowercase hex of the ed25519 signature over
    /// `challenge_bytes || pubkey_bytes`.
    pub signature: String,
}

async fn self_enroll(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
    Json(body): Json<SelfEnrollBody>,
) -> Response {
    // 1. Parse the kind. Reject unknown values rather than silently
    //    coercing them so a malformed client gets a clear error.
    let kind = match PeerKind::parse(body.kind.as_str()) {
        Some(k) => k,
        None => return (StatusCode::BAD_REQUEST, "kind must be 'web' or 'daemon'").into_response(),
    };

    // 2. Parse pubkey hex.
    let pubkey = match PublicKey::from_hex(body.pubkey.trim()) {
        Ok(p) => p,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid pubkey hex").into_response(),
    };

    // 3. Look up the challenge bound to this user, verify it's
    //    still live. `None` covers missing/expired/wrong-user — we
    //    surface them all as the same client error.
    let challenge_row =
        match EnrollChallenge::find_live_for_user(&body.challenge, user.id(), &state.db).await {
            Ok(Some(r)) => r,
            Ok(None) => {
                return (StatusCode::BAD_REQUEST, "challenge not valid").into_response();
            }
            Err(e) => {
                tracing::error!("challenge lookup failed: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "challenge lookup failed")
                    .into_response();
            }
        };
    let challenge_bytes = match challenge_row.challenge_bytes() {
        Some(b) => b,
        None => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "challenge row corrupt").into_response()
        }
    };

    // 4. Verify the signature against `pubkey`. Signed payload is
    //    `challenge_bytes || pubkey_bytes`. Same byte layout the
    //    browser-side `device-setup.js` and the future daemon CLI
    //    will use.
    let sig_bytes = match decode_signature(&body.signature) {
        Some(b) => b,
        None => return (StatusCode::BAD_REQUEST, "invalid signature hex").into_response(),
    };
    let mut payload = Vec::with_capacity(challenge_bytes.len() + 32);
    payload.extend_from_slice(&challenge_bytes);
    payload.extend_from_slice(&pubkey.to_bytes());
    if pubkey.verify_bytes(&payload, &sig_bytes).is_err() {
        return (StatusCode::UNAUTHORIZED, "signature does not verify").into_response();
    }

    // 5. Idempotent path: if THIS user already has THIS pubkey
    //    enrolled, treat the call as a successful no-op. Lets
    //    `zim hub login` re-run without manual cleanup — the daemon
    //    just gets a fresh bearer + reuses the existing row.
    //
    //    A different user attempting the same pubkey still hits
    //    the PK constraint below and returns 409 (claim conflict).
    //    A web-key conflict (this user already has a web row,
    //    different pubkey) also stays 409.
    match UserPeer::user_owns_pubkey(user.id(), &pubkey, &state.db).await {
        Ok(true) => {
            tracing::info!(
                user = user.email(),
                pubkey = %pubkey.to_hex(),
                "device re-enrollment is a no-op (already owned)"
            );
            // Still consume the challenge so a leaked signed payload
            // isn't replayable for a *new* enrollment elsewhere.
            if let Err(e) = EnrollChallenge::consume(&body.challenge, &state.db).await {
                tracing::warn!("consume challenge: {e}");
            }
            return StatusCode::OK.into_response();
        }
        Ok(false) => {}
        Err(e) => {
            tracing::error!("user_owns_pubkey check: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "lookup failed").into_response();
        }
    }

    // Label is optional: a web key is the account's single master identity
    // and needs none; daemons may set one to tell devices apart. Empty → NULL.
    let label = body.label.trim();
    let label = (!label.is_empty()).then_some(label);
    match UserPeer::create(user.id(), &pubkey, label, kind, &state.db).await {
        Ok(_) => {}
        Err(sqlx::Error::Database(e)) if e.message().contains("UNIQUE constraint failed") => {
            return (StatusCode::CONFLICT, "already enrolled").into_response();
        }
        Err(e) => {
            tracing::error!("user_peers insert: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "enrollment failed").into_response();
        }
    }

    // 6. Consume the challenge. Logged-only on failure — the
    //    enrollment already committed, so we don't bounce the
    //    client; the challenge will be expired by the periodic
    //    cleanup at worst.
    if let Err(e) = EnrollChallenge::consume(&body.challenge, &state.db).await {
        tracing::warn!("consume challenge: {e}");
    }

    tracing::info!(
        user = user.email(),
        pubkey = %pubkey.to_hex(),
        kind = kind.as_str(),
        "device enrolled"
    );
    StatusCode::CREATED.into_response()
}

fn decode_signature(s: &str) -> Option<[u8; 64]> {
    let s = s.trim();
    if s.len() != 128 {
        return None;
    }
    let mut out = [0u8; 64];
    hex::decode_to_slice(s, &mut out).ok()?;
    Some(out)
}
