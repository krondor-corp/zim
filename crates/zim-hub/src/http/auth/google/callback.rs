//! `GET /auth/google/callback` — exchange the auth code, validate
//! the ID-token claims, then hand the verified email to
//! [`mint_session`] (shared with the dev-trust login).
//!
//! ID-token signature: trusted via TLS from Google's token endpoint;
//! we still audience+issuer-check to catch a misconfigured client_id.

use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use http::StatusCode;
use jsonwebtoken::{DecodingKey, Validation};
use serde::Deserialize;

use crate::http::auth::login_path;
use crate::http::auth::session::{mint_session, Authority};
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

pub async fn handler(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let login = login_path(&state.auth);
    // Only mounted in Google mode, so the client is always there.
    let Some((client_id, client_secret)) = state.auth.google_client() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let redirect_uri = state.auth.callback_url();

    let id_token = match exchange_code(client_id, client_secret, &redirect_uri, &query.code).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("token exchange failed: {e}");
            return Redirect::to(login).into_response();
        }
    };

    let id_claims = match decode_id_token(&id_token, client_id) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("id_token decode failed: {e}");
            return Redirect::to(login).into_response();
        }
    };

    if !id_claims.email_verified {
        tracing::warn!(email = %id_claims.email, "google reports email_verified=false");
        return Redirect::to(login).into_response();
    }

    let name = id_claims.name.unwrap_or_default();
    let authority = Authority::Google { sub: id_claims.sub };
    match mint_session(&state, &id_claims.email, &name, authority).await {
        Ok(r) => r,
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
