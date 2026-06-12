//! Multi-user Google OAuth gate.
//!
//! Three extractors layered by access level:
//!
//! - [`OptionalUser`] — never rejects. Reads the session cookie if
//!   present and looks up the matching DB row.
//! - [`RequireUser`] — handler runs only for authorized (or admin)
//!   users. Anonymous → redirect to login (401 for non-HTML).
//!   Authenticated-but-pending → redirect to `/auth/pending`.
//! - [`RequireAdmin`] — `is_admin` only. Anonymous → login,
//!   non-admin signed-in → 403.
//!
//! Session cookies are HS256 JWTs. Email is the load-bearing claim;
//! the user row is looked up fresh on every gated request so role
//! changes take effect immediately.

pub mod device;
pub mod google;
pub mod jwt;
pub mod logout;

use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::post;
use axum::Router;
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

use crate::database::models::{User, UserPeer};
use crate::state::AppState;

pub const LOGIN_PATH: &str = "/auth/google/login";
pub const PENDING_PATH: &str = "/auth/pending";
pub const ONBOARDING_PATH: &str = "/app/onboarding";
pub const SESSION_COOKIE: &str = "session";

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .nest("/google", google::router())
        .nest("/device", device::router(state.clone()))
        .route("/logout", post(logout::handler))
        .route("/pending", axum::routing::get(pending_page))
        .with_state(state)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub email: String,
    pub name: String,
    pub iat: usize,
    pub exp: usize,
}

pub struct OptionalUser(pub Option<User>);

#[async_trait]
impl FromRequestParts<AppState> for OptionalUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(OptionalUser(resolve_user(parts, state).await))
    }
}

pub struct RequireUser(pub User);

#[async_trait]
impl FromRequestParts<AppState> for RequireUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Some(user) = resolve_user(parts, state).await else {
            return Err(unauthorized(parts));
        };
        if !user.can_use_app() {
            return Err(if wants_html(parts) {
                Redirect::to(PENDING_PATH).into_response()
            } else {
                StatusCode::FORBIDDEN.into_response()
            });
        }
        Ok(RequireUser(user))
    }
}

/// Authenticated, authorized, AND has at least one device
/// registered. Used by every "real" workspace page so a fresh user
/// can't bypass the onboarding flow by typing a URL.
///
/// Pre-onboarding users (zero devices) get redirected to
/// [`ONBOARDING_PATH`] for HTML clients, 403 for JSON clients.
///
/// Routes the user needs in order to *finish* onboarding (the
/// `/app/devices` page, the onboarding page itself) use the plain
/// [`RequireUser`] instead so they don't loop.
pub struct RequireOnboardedUser(pub User);

#[async_trait]
impl FromRequestParts<AppState> for RequireOnboardedUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Some(user) = resolve_user(parts, state).await else {
            return Err(unauthorized(parts));
        };
        if !user.can_use_app() {
            return Err(if wants_html(parts) {
                Redirect::to(PENDING_PATH).into_response()
            } else {
                StatusCode::FORBIDDEN.into_response()
            });
        }
        // Admin bypass: an admin without devices can still walk into
        // /_admin to manage other users. Otherwise the very first
        // admin gets stuck on the onboarding wall with no way to
        // promote anyone else.
        if !user.is_admin() {
            match UserPeer::user_has_web_key(user.id(), &state.db).await {
                Ok(true) => {}
                Ok(false) => {
                    return Err(if wants_html(parts) {
                        Redirect::to(ONBOARDING_PATH).into_response()
                    } else {
                        StatusCode::FORBIDDEN.into_response()
                    });
                }
                Err(e) => {
                    tracing::error!("device-count lookup failed: {e}");
                    return Err(StatusCode::INTERNAL_SERVER_ERROR.into_response());
                }
            }
        }
        Ok(RequireOnboardedUser(user))
    }
}

pub struct RequireAdmin(pub User);

#[async_trait]
impl FromRequestParts<AppState> for RequireAdmin {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Some(user) = resolve_user(parts, state).await else {
            return Err(unauthorized(parts));
        };
        if !user.is_admin() {
            return Err(StatusCode::FORBIDDEN.into_response());
        }
        Ok(RequireAdmin(user))
    }
}

/// Resolve the requesting user from either an Ed25519 self-signed
/// JWT bearer (`Authorization: Bearer <jwt>`, daemons) or the
/// HS256 session cookie (browsers). Bearer wins when both are
/// present — daemons hitting the API shouldn't accidentally pick
/// up a stale cookie.
async fn resolve_user(parts: &Parts, state: &AppState) -> Option<User> {
    if let Some(token) = bearer_token(parts) {
        return resolve_user_by_jwt(&token, state).await;
    }
    let claims = extract_claims(parts, &state.auth.service_secret)?;
    match User::find_by_email(&claims.email, &state.db).await {
        Ok(u) => {
            if u.is_none() {
                tracing::warn!(
                    email = %claims.email,
                    "session valid but user row missing"
                );
            }
            u
        }
        Err(e) => {
            tracing::error!(email = %claims.email, "user lookup failed: {e}");
            None
        }
    }
}

/// Verify a self-signed Ed25519 JWT against the `user_peers` table.
///
/// Sequence:
///
/// 1. Parse the JWT (alg=EdDSA, `kid` = 64-char pubkey hex).
/// 2. Look up the pubkey in `user_peers` to find its owner. If the
///    pubkey isn't enrolled, the JWT is invalid no matter how good
///    the signature is.
/// 3. Verify the signature over `header.payload` against the
///    pubkey.
/// 4. Check `iat`/`exp`/`aud` claims.
/// 5. Touch `user_peers.last_seen_at` (best-effort).
async fn resolve_user_by_jwt(token: &str, state: &AppState) -> Option<User> {
    let verified = match jwt::verify(token, &state.auth.host_name) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!("jwt verify: {e}");
            return None;
        }
    };
    let peer = match UserPeer::find_by_pubkey(&verified.pubkey, &state.db).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            tracing::debug!(
                pubkey = %verified.pubkey.to_hex(),
                "jwt signed by unenrolled pubkey"
            );
            return None;
        }
        Err(e) => {
            tracing::error!("user_peers lookup: {e}");
            return None;
        }
    };
    let user = User::find_by_id(peer.user_id(), &state.db)
        .await
        .ok()
        .flatten()?;
    Some(user)
}

fn bearer_token(parts: &Parts) -> Option<String> {
    let auth = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let token = auth.strip_prefix("Bearer ")?.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

fn unauthorized(parts: &Parts) -> Response {
    if wants_html(parts) {
        Redirect::to(LOGIN_PATH).into_response()
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

fn wants_html(parts: &Parts) -> bool {
    parts
        .headers
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.contains("text/html"))
}

fn extract_claims(parts: &Parts, secret: &str) -> Option<Claims> {
    let cookie_header = parts
        .headers
        .get(axum::http::header::COOKIE)?
        .to_str()
        .ok()?;
    let token = cookie_header
        .split(';')
        .filter_map(|c| {
            let mut kv = c.trim().splitn(2, '=');
            let name = kv.next()?.trim();
            let value = kv.next()?.trim();
            (name == SESSION_COOKIE).then(|| value.to_string())
        })
        .next()?;
    if secret.is_empty() {
        return None;
    }
    let decoded = decode::<Claims>(
        &token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(jsonwebtoken::Algorithm::HS256),
    )
    .ok()?;
    Some(decoded.claims)
}

async fn pending_page(OptionalUser(user): OptionalUser) -> Response {
    let email = user.as_ref().map(|u| u.email()).unwrap_or("you");
    let body = format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>access pending — zim-hub</title>
  <link rel="stylesheet" href="/static/style.css">
</head>
<body>
  <header><h1>zim-hub</h1></header>
  <main>
    <h2>access pending</h2>
    <p>You're signed in as <strong>{email}</strong>, but your account hasn't been authorized for this hub yet. An admin needs to approve you.</p>
    <p><form method="post" action="/auth/logout" style="display:inline">
      <button type="submit" class="link-button">sign out</button>
    </form></p>
  </main>
</body>
</html>"#
    );
    axum::response::Html(body).into_response()
}
