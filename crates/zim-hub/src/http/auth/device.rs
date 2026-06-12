//! `/auth/device/*` — browser side of the device-code flow.
//!
//! Two routes:
//!
//! - `GET  /auth/device[?code=ABCD-EFGH]` — the page the daemon
//!   tells the user to open. Pre-fills the code from the query
//!   string when present. RequireUser-gated; unauthenticated users
//!   bounce through the standard login redirect first.
//! - `POST /auth/device/approve` — same gate. Just stamps
//!   `user_id` + `approved_at` on the grant. The daemon's next
//!   poll, signed by the matching identity key, will atomically
//!   enroll itself into `user_peers`.

use askama::Template;
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use http::StatusCode;
use serde::Deserialize;

use crate::database::models::DeviceCodeGrant;
use crate::http::auth::RequireUser;
use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(page))
        .route("/approve", post(approve))
        .with_state(state)
}

#[derive(Template)]
#[template(path = "pages/device-pair.html")]
struct PagePending<'a> {
    hub_version: &'static str,
    user_email: String,
    is_admin: bool,
    active_nav: &'static str,
    code: &'a str,
    state: PairState<'a>,
}

#[derive(Debug)]
enum PairState<'a> {
    /// User hasn't typed a code yet.
    NeedsCode,
    /// Grant found, pending approval. Render the Approve button
    /// alongside the pubkey + label so the user can verify before
    /// clicking.
    Pending { label: &'a str, pubkey: &'a str },
    /// Code typed but the row is expired / missing / already taken.
    Invalid { msg: &'a str },
    /// Just approved. The daemon should pick it up via poll any
    /// second now.
    Approved,
}

#[derive(Debug, Deserialize)]
pub struct PageQuery {
    pub code: Option<String>,
    #[serde(default)]
    pub approved: bool,
}

async fn page(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
    Query(q): Query<PageQuery>,
) -> Response {
    let code = q.code.unwrap_or_default();
    let trimmed = code.trim();
    let pair_state = if q.approved {
        PairState::Approved
    } else if trimmed.is_empty() {
        PairState::NeedsCode
    } else {
        match DeviceCodeGrant::find(trimmed, &state.db).await {
            Ok(Some(g)) if g.is_expired() => PairState::Invalid {
                msg: "Code expired. Run `zim login` again to mint a fresh one.",
            },
            Ok(Some(g)) if g.is_approved() => PairState::Invalid {
                msg: "Code already approved. The daemon should have picked it up.",
            },
            Ok(Some(g)) => PairState::Pending {
                label: leak_str(g.label()),
                pubkey: leak_str(g.pubkey_hex()),
            },
            Ok(None) => PairState::Invalid {
                msg: "Code not found. Check for typos and try again.",
            },
            Err(e) => {
                tracing::error!("device-pair lookup: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "lookup failed").into_response();
            }
        }
    };

    let tmpl = PagePending {
        hub_version: env!("CARGO_PKG_VERSION"),
        user_email: user.email().to_string(),
        is_admin: user.is_admin(),
        active_nav: "",
        code: trimmed,
        state: pair_state,
    };
    Html(tmpl.render().unwrap_or_default()).into_response()
}

/// String-leak so the askama template (which holds `&str`) can
/// outlive the local `DeviceCodeGrant`. The label/pubkey were
/// bounded by the daemon at start-time and are small + finite. The
/// leak is per-request, on a request that already touched the DB.
fn leak_str(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

#[derive(Debug, Deserialize)]
pub struct ApproveForm {
    pub code: String,
}

async fn approve(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
    Form(form): Form<ApproveForm>,
) -> Response {
    let code = form.code.trim();
    let grant = match DeviceCodeGrant::find(code, &state.db).await {
        Ok(Some(g)) => g,
        Ok(None) => {
            return redirect_with_state(code, "Code not found.");
        }
        Err(e) => {
            tracing::error!("approve lookup: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "lookup failed").into_response();
        }
    };
    if grant.is_expired() {
        return redirect_with_state(code, "Code expired. Mint a fresh one with `zim login`.");
    }
    if grant.is_approved() {
        return Redirect::to(&format!("/auth/device?code={code}&approved=true")).into_response();
    }

    match DeviceCodeGrant::approve(code, user.id(), &state.db).await {
        Ok(true) => {
            tracing::info!(
                user = user.email(),
                label = grant.label(),
                pubkey = grant.pubkey_hex(),
                "device-code approved"
            );
            Redirect::to(&format!("/auth/device?code={code}&approved=true")).into_response()
        }
        Ok(false) => redirect_with_state(code, "Approval failed (code consumed or expired)."),
        Err(e) => {
            tracing::error!("approve update: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "approval failed").into_response()
        }
    }
}

fn redirect_with_state(code: &str, msg: &str) -> Response {
    let encoded = urlencoding::encode(msg);
    Redirect::to(&format!("/auth/device?code={code}&err={encoded}")).into_response()
}
