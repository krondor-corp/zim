//! `POST /auth/dev/login` — trust-mode login: mints the session for
//! whatever email the SPA's sign-in page posted. Everyone is admin.
//! Exists only in `--features dev-login` builds and only when
//! `ZIM_HUB_AUTH` lists `dev-trust`; the router is not even mounted
//! otherwise.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Form, Router};
use serde::Deserialize;

use crate::http::auth::session::{mint_session, Authority};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/login", post(submit))
}

#[derive(Deserialize)]
struct Submit {
    email: String,
}

async fn submit(State(state): State<AppState>, Form(f): Form<Submit>) -> Response {
    let email = f.email.trim().to_lowercase();
    if email.is_empty() {
        return (StatusCode::BAD_REQUEST, "email required").into_response();
    }
    let name = email.split('@').next().unwrap_or("dev").to_string();
    match mint_session(&state, &email, &name, Authority::DevTrust).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("dev sign-in failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
