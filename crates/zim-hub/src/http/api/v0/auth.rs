//! `/api/v0/auth/*` — device-code OAuth flow for daemons.
//!
//! Two daemon-callable public endpoints. The browser-side approval
//! lives at `/auth/device/*` (see [`crate::http::auth::device`]).
//!
//! ```text
//!     daemon                                 browser           hub
//!       |   POST /api/v0/auth/device-code/start                  |
//!       |    { pubkey, label }                                   |
//!       | ----------------------------------------------------> |
//!       | <-------- { code, verification_url, expires_at } ---- |
//!       |                                                       |
//!       |   user opens verification_url:                        |
//!       |       GET /auth/device?code=…                         |
//!       |                                                       |
//!       |                          (signs in if needed)         |
//!       |                          verifies pubkey + label,     |
//!       |                          clicks Approve →             |
//!       |                          POST /auth/device/approve    |
//!       |                                                       |
//!       |   POST /api/v0/auth/device-code/poll                  |
//!       |    { code, signature }                                |
//!       |    signature = sign(code_bytes || pubkey_bytes)       |
//!       | ----------------------------------------------------> |
//!       | <----- 200 {ok}  /  202 pending  /  410 gone --------- |
//! ```
//!
//! 200 from `poll` means the daemon's `user_peers` row already
//! exists (atomic with the poll). The daemon authenticates every
//! subsequent request by signing a short-lived Ed25519 JWT with the
//! same identity key — there's no long-lived bearer token to leak
//! or rotate.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use zim_crypto::PublicKey;

use crate::database::models::{DeviceCodeGrant, PeerKind, UserPeer};
use crate::http::auth::RequireUser;
use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/device-code/start", post(device_code_start))
        .route("/device-code/poll", post(device_code_poll))
        // Browser approval — consumed by the SPA `/device` page.
        .route("/device-code/:code", get(device_code_info))
        .route("/device-code/:code/approve", post(device_code_approve))
        .with_state(state)
}

#[derive(Debug, Deserialize)]
pub struct StartBody {
    /// 64-char lowercase hex of the daemon's ed25519 public key. The
    /// daemon commits to it here so the approve page can render it
    /// for the user and the poll signature has a fixed target.
    pub pubkey: String,
    /// Free-form display label for the device. Shown on the approve
    /// page next to the pubkey. Hostname is a good default.
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct StartResponse {
    /// 9-char human-typable code (`ABCD-EFGH`). The daemon prints
    /// it for the user to verify on the approve page.
    pub code: String,
    /// Full URL the daemon prints / opens for the user.
    pub verification_url: String,
    pub expires_at: String,
    /// Suggested poll interval. The hub doesn't currently rate
    /// limit, but daemons should still back off if servers grow
    /// strict later.
    pub poll_interval_secs: u64,
}

async fn device_code_start(State(state): State<AppState>, Json(body): Json<StartBody>) -> Response {
    // Validate the pubkey here so a malformed daemon gets a clear
    // 400 instead of a silent mismatch at poll time.
    if PublicKey::from_hex(body.pubkey.trim()).is_err() {
        return (StatusCode::BAD_REQUEST, "invalid pubkey hex").into_response();
    }
    let label = body.label.trim();
    match DeviceCodeGrant::create(body.pubkey.trim(), label, &state.db).await {
        Ok(grant) => {
            let base = state.auth.host_name.trim_end_matches('/');
            let url = format!("{base}/device?code={}", grant.code());
            Json(StartResponse {
                code: grant.code().to_string(),
                verification_url: url,
                expires_at: grant.expires_at().to_string(),
                poll_interval_secs: 5,
            })
            .into_response()
        }
        Err(e) => {
            tracing::error!("device-code start: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not mint code").into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct PollBody {
    pub code: String,
    /// 128-char lowercase hex of the ed25519 signature over
    /// `code_bytes || pubkey_bytes`. Optional: when omitted, this
    /// is treated as a "status check" poll and the response is
    /// 202 pending / 410 gone, without ever enrolling.
    #[serde(default)]
    pub signature: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PollOk {
    pub ok: bool,
    /// Echoes the pubkey that was just enrolled. Lets the daemon
    /// double-check the hub agrees on identity before persisting
    /// session state.
    pub pubkey: String,
}

async fn device_code_poll(State(state): State<AppState>, Json(body): Json<PollBody>) -> Response {
    let code = body.code.trim();
    let grant = match DeviceCodeGrant::find(code, &state.db).await {
        Ok(Some(g)) => g,
        Ok(None) => return (StatusCode::GONE, "code not found").into_response(),
        Err(e) => {
            tracing::error!("device-code poll lookup: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "lookup failed").into_response();
        }
    };
    if grant.is_expired() {
        let _ = DeviceCodeGrant::consume(code, &state.db).await;
        return (StatusCode::GONE, "code expired").into_response();
    }
    let Some(user_id) = grant.user_id() else {
        return (StatusCode::ACCEPTED, "pending").into_response();
    };

    // Pending-signature status check: caller just wants to know
    // whether the grant has been approved yet. Don't enroll.
    let Some(sig_hex) = body.signature.as_deref() else {
        return (StatusCode::ACCEPTED, "approved; resend with signature").into_response();
    };

    // Verify the daemon possesses the pubkey that was committed at
    // start-time. Payload is `code_bytes || pubkey_bytes` — the
    // code is single-use (we consume on success), and the pubkey
    // commits the signer.
    let pubkey = match PublicKey::from_hex(grant.pubkey_hex()) {
        Ok(p) => p,
        Err(_) => {
            tracing::error!("grant {code} has corrupt pubkey on row");
            return (StatusCode::INTERNAL_SERVER_ERROR, "corrupt grant").into_response();
        }
    };
    let sig_bytes = match decode_signature(sig_hex) {
        Some(b) => b,
        None => return (StatusCode::BAD_REQUEST, "invalid signature hex").into_response(),
    };
    let mut payload = Vec::with_capacity(code.len() + 32);
    payload.extend_from_slice(code.as_bytes());
    payload.extend_from_slice(&pubkey.to_bytes());
    if pubkey.verify_bytes(&payload, &sig_bytes).is_err() {
        return (StatusCode::UNAUTHORIZED, "signature does not verify").into_response();
    }

    // Idempotent enrollment. If THIS user already owns this pubkey
    // (rare: re-poll after the grant was already consumed but the
    // first response was lost in flight), treat as success. The PK
    // collision below would also catch this but returning 200
    // explicitly here gives a clearer audit trail.
    match UserPeer::user_owns_pubkey(user_id, &pubkey, &state.db).await {
        Ok(true) => {
            let _ = DeviceCodeGrant::consume(code, &state.db).await;
            return Json(PollOk {
                ok: true,
                pubkey: grant.pubkey_hex().to_string(),
            })
            .into_response();
        }
        Ok(false) => {}
        Err(e) => {
            tracing::error!("user_owns_pubkey: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "lookup failed").into_response();
        }
    }

    let label = grant.label().trim();
    let label = (!label.is_empty()).then_some(label);
    match UserPeer::create(user_id, &pubkey, label, PeerKind::Daemon, &state.db).await {
        Ok(_) => {}
        Err(sqlx::Error::Database(e)) if e.message().contains("UNIQUE constraint failed") => {
            // Pubkey is claimed by a different user. Don't leak who.
            return (StatusCode::CONFLICT, "pubkey already claimed").into_response();
        }
        Err(e) => {
            tracing::error!("user_peers insert: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "enrollment failed").into_response();
        }
    }

    if let Err(e) = DeviceCodeGrant::consume(code, &state.db).await {
        tracing::warn!("consume grant on poll: {e}");
    }
    tracing::info!(
        user_id = %user_id,
        pubkey = grant.pubkey_hex(),
        "device enrolled via device-code"
    );
    Json(PollOk {
        ok: true,
        pubkey: grant.pubkey_hex().to_string(),
    })
    .into_response()
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

// ─── Browser approval (SPA `/device` page) ────────────────────────────

// Shared wire type — mirrored by `zim_api::hub::GrantInfoRequest`.
use zim_api::hub::GrantInfo;

/// `GET /api/v0/auth/device-code/:code` — the grant a user is about to
/// approve. RequireUser so only a signed-in human can read it.
async fn device_code_info(
    State(state): State<AppState>,
    RequireUser(_user): RequireUser,
    Path(code): Path<String>,
) -> Response {
    match DeviceCodeGrant::find(code.trim(), &state.db).await {
        Ok(Some(g)) => {
            let status = if g.is_expired() {
                "expired"
            } else if g.is_approved() {
                "approved"
            } else {
                "pending"
            };
            Json(GrantInfo {
                status: status.to_string(),
                label: g.label().to_string(),
                pubkey: g.pubkey_hex().to_string(),
            })
            .into_response()
        }
        Ok(None) => Json(GrantInfo {
            status: "not_found".to_string(),
            label: String::new(),
            pubkey: String::new(),
        })
        .into_response(),
        Err(e) => {
            tracing::error!("device-code info: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "lookup failed").into_response()
        }
    }
}

/// `POST /api/v0/auth/device-code/:code/approve` — stamp `user_id` +
/// `approved_at`; the daemon's next signed poll enrolls itself.
async fn device_code_approve(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
    Path(code): Path<String>,
) -> Response {
    let code = code.trim();
    let grant = match DeviceCodeGrant::find(code, &state.db).await {
        Ok(Some(g)) => g,
        Ok(None) => return (StatusCode::NOT_FOUND, "code not found").into_response(),
        Err(e) => {
            tracing::error!("device-code approve lookup: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "lookup failed").into_response();
        }
    };
    if grant.is_expired() {
        return (StatusCode::GONE, "code expired").into_response();
    }
    if grant.is_approved() {
        return StatusCode::NO_CONTENT.into_response();
    }
    match DeviceCodeGrant::approve(code, user.id(), &state.db).await {
        Ok(true) => {
            tracing::info!(
                user = user.email(),
                label = grant.label(),
                pubkey = grant.pubkey_hex(),
                "device-code approved"
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => (StatusCode::CONFLICT, "code consumed or expired").into_response(),
        Err(e) => {
            tracing::error!("device-code approve: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "approval failed").into_response()
        }
    }
}
