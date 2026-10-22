//! `GET /auth/google/callback` — check `state` against this browser's
//! cookie (see [`super::state`]), exchange the auth code, validate the
//! ID-token claims, then hand the verified email to [`mint_session`]
//! (shared with the dev-trust login), landing on the carried `next`.
//!
//! ID-token signature: trusted via TLS from Google's token endpoint;
//! we still audience+issuer-check to catch a misconfigured client_id.

use axum::extract::{Query, State};
use axum::http::header::SET_COOKIE;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use http::StatusCode;
use jsonwebtoken::{DecodingKey, Validation};
use serde::Deserialize;

use super::state as oauth_state;
use crate::http::auth::session::{mint_session, Authority};
use crate::http::auth::{return_to, LOGIN_PATH};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    #[serde(default)]
    pub state: String,
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

pub async fn handler(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
    headers: HeaderMap,
) -> Response {
    // Only mounted when Google is an enabled provider.
    let Some(google) = state.auth.google() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (client_id, client_secret) = (&google.client_id, &google.client_secret);
    let redirect_uri = state.auth.callback_url();
    let secure = state.auth.host_name.starts_with("https://");

    let Some(next) = oauth_state::finish(&query.state, &headers) else {
        tracing::warn!("oauth state missing or doesn't match this browser's cookie");
        return Redirect::to(LOGIN_PATH).into_response();
    };
    let retry = return_to::login_url(&next);

    let id_token = match exchange_code(client_id, client_secret, &redirect_uri, &query.code).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("token exchange failed: {e}");
            return Redirect::to(&retry).into_response();
        }
    };

    let id_claims = match decode_id_token(&id_token, client_id) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("id_token decode failed: {e}");
            return Redirect::to(&retry).into_response();
        }
    };

    if !id_claims.email_verified {
        tracing::warn!(email = %id_claims.email, "google reports email_verified=false");
        return Redirect::to(&retry).into_response();
    }

    let name = id_claims.name.unwrap_or_default();
    let authority = Authority::Google { sub: id_claims.sub };
    match mint_session(&state, &id_claims.email, &name, authority, &next).await {
        Ok(mut r) => {
            if let Ok(clear) = oauth_state::clear_cookie(secure).parse() {
                r.headers_mut().append(SET_COOKIE, clear);
            }
            r
        }
        Err(e) => {
            tracing::error!("google sign-in failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "sign-in failed").into_response()
        }
    }
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
