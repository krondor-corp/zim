//! `/auth/dev/*` — trust-mode login. GET renders a one-field form,
//! POST mints the session for whatever email was typed. Everyone is
//! admin. Exists only in `--features dev-login` builds and only when
//! `ZIM_HUB_AUTH=dev-trust`; the router is not even mounted otherwise.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Form, Router};
use serde::Deserialize;

use crate::http::auth::session::{mint_session, Authority};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/login", get(form).post(submit))
}

async fn form(State(state): State<AppState>) -> Response {
    let default = state
        .auth
        .admin_emails
        .first()
        .map(|e| html_escape(e))
        .unwrap_or_default();
    Html(format!(
        r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>zim hub — dev login</title></head>
<body style="font-family:system-ui;max-width:28rem;margin:4rem auto">
  <div style="background:#b00020;color:#fff;padding:.5rem 1rem;font-weight:600">
    DEV TRUST MODE — any email signs in as admin
  </div>
  <form method="post" style="margin-top:1.5rem">
    <label>Email <input name="email" type="email" value="{default}" autofocus required
      style="display:block;width:100%;padding:.5rem;margin:.5rem 0 1rem"></label>
    <button>Sign in</button>
  </form>
</body>
</html>"#
    ))
    .into_response()
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

/// The pre-fill comes from operator env, but it lands in an attribute.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
