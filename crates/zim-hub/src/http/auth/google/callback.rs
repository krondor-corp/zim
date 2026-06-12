//! `GET /auth/google/callback` — exchange the auth code, validate
//! the ID-token claims, find-or-create the [`User`] row (bootstrap
//! admins via `admin_emails`), mint a session cookie, redirect home.
//!
//! ID-token signature: trusted via TLS from Google's token endpoint;
//! we still audience+issuer-check to catch a misconfigured client_id.

use axum::extract::{Query, State};
use axum::http::header::SET_COOKIE;
use axum::response::{IntoResponse, Redirect, Response};
use chrono::{Duration, Utc};
use http::StatusCode;
use jsonwebtoken::{encode, DecodingKey, EncodingKey, Header, Validation};
use serde::Deserialize;

use crate::database::models::User;
use crate::http::auth::{Claims, LOGIN_PATH, SESSION_COOKIE};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: String,
}

#[derive(Deserialize)]
struct IdTokenClaims {
    sub: String,
    email: String,
    #[serde(default)]
    email_verified: bool,
    #[serde(default)]
    name: Option<String>,
}

const COOKIE_MAX_AGE_SECS: i64 = 60 * 60 * 24 * 7;

pub async fn handler(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let redirect_uri = state.auth.callback_url();

    let id_token = match exchange_code(
        &state.auth.google_o_auth_client_id,
        &state.auth.google_o_auth_client_secret,
        &redirect_uri,
        &query.code,
    )
    .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("token exchange failed: {e}");
            return Redirect::to(LOGIN_PATH).into_response();
        }
    };

    let id_claims = match decode_id_token(&id_token, &state.auth.google_o_auth_client_id) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("id_token decode failed: {e}");
            return Redirect::to(LOGIN_PATH).into_response();
        }
    };

    if !id_claims.email_verified {
        tracing::warn!(email = %id_claims.email, "google reports email_verified=false");
        return Redirect::to(LOGIN_PATH).into_response();
    }

    let name = id_claims.name.unwrap_or_default();
    let user = match find_or_create_user(&state, &id_claims.email, &name).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("user find_or_create failed: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "user provisioning failed",
            )
                .into_response();
        }
    };

    let now = Utc::now();
    let exp = now + Duration::seconds(COOKIE_MAX_AGE_SECS);
    let session_claims = Claims {
        sub: id_claims.sub,
        email: user.email().to_string(),
        name: user.name().to_string(),
        iat: now.timestamp() as usize,
        exp: exp.timestamp() as usize,
    };

    let token = match encode(
        &Header::default(),
        &session_claims,
        &EncodingKey::from_secret(state.auth.service_secret.as_bytes()),
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("session token encode failed: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "session error").into_response();
        }
    };

    let secure = state.auth.host_name.starts_with("https://");
    let cookie = build_cookie(&token, secure, COOKIE_MAX_AGE_SECS);

    tracing::info!(
        email = user.email(),
        role = %user.role().label(),
        "user signed in"
    );

    // Land authenticated users in the workspace, not on the public
    // marketing page.
    ([(SET_COOKIE, cookie)], Redirect::to("/app")).into_response()
}

/// Look up the user by email or insert a fresh row. Emails on the
/// configured `admin_emails` allow-list are inserted with both
/// `is_admin` and `is_authorized` true — the bootstrap path so the
/// first admin doesn't need a manual SQL hop. Subsequent role
/// changes happen through `/_admin`.
async fn find_or_create_user(
    state: &AppState,
    email: &str,
    name: &str,
) -> Result<User, sqlx::Error> {
    if let Some(existing) = User::find_by_email(email, &state.db).await? {
        return Ok(existing);
    }
    let bootstrap_admin = state.auth.is_bootstrap_admin(email);
    User::create(email, name, bootstrap_admin, bootstrap_admin, &state.db).await
}

async fn exchange_code(
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    code: &str,
) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let res = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<TokenResponse>()
        .await?;
    Ok(res.id_token)
}

fn decode_id_token(id_token: &str, client_id: &str) -> anyhow::Result<IdTokenClaims> {
    let mut validation = Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.insecure_disable_signature_validation();
    validation.set_audience(&[client_id]);
    validation.set_issuer(&["https://accounts.google.com", "accounts.google.com"]);

    let data = jsonwebtoken::decode::<IdTokenClaims>(
        id_token,
        &DecodingKey::from_secret(&[]),
        &validation,
    )?;
    Ok(data.claims)
}

fn build_cookie(token: &str, secure: bool, max_age_secs: i64) -> String {
    let secure_attr = if secure { "; Secure" } else { "" };
    format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}{secure_attr}"
    )
}
