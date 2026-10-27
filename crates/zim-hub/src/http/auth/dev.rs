//! `/auth/dev/login` — trust-mode login. GET renders the email form
//! (reached from the [`super::login`] chooser), POST mints the session
//! for whatever email was typed and returns to `next`. Everyone is
//! admin. Exists only in `--features dev-login` builds and only when
//! `ZIM_HUB_AUTH` lists `dev-trust`; the router is not even mounted
//! otherwise.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::{Form, Router};
use serde::Deserialize;

use crate::http::auth::login::{escape, shell, LoginError};
use crate::http::auth::return_to;
use crate::http::auth::session::{mint_session, Authority};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/login", get(form).post(submit))
}

#[derive(Deserialize)]
struct FormQuery {
    next: Option<String>,
}

async fn form(Query(query): Query<FormQuery>) -> Response {
    let next = return_to::sanitize(query.next.as_deref());
    Html(form_page(next)).into_response()
}

fn form_page(next: &str) -> String {
    shell(
        "Dev login",
        &format!(
            r#"<form method="post" action="/auth/dev/login">
          <div class="error" style="margin-bottom:0.75rem;">⚠ Development only — if you see this in production, something is wrong. Fix immediately.</div>
          <input type="hidden" name="next" value="{next_attr}">
          <div class="field">
            <label for="email">Email</label>
            <input id="email" type="email" name="email" required autofocus>
          </div>
          <button type="submit" class="btn btn-primary" style="width:100%;">Sign in</button>
        </form>
        <p class="dim" style="text-align:center;margin-top:1rem;"><a href="{back}">Other sign-in options</a></p>"#,
            next_attr = escape(next),
            back = escape(&return_to::login_url(next)),
        ),
    )
}

#[derive(Deserialize)]
struct Submit {
    email: String,
    #[serde(default)]
    next: Option<String>,
}

async fn submit(State(state): State<AppState>, Form(f): Form<Submit>) -> Response {
    let email = f.email.trim().to_lowercase();
    if email.is_empty() {
        return (StatusCode::BAD_REQUEST, "email required").into_response();
    }
    let name = email.split('@').next().unwrap_or("dev").to_string();
    let next = return_to::sanitize(f.next.as_deref());
    match mint_session(&state, &email, &name, Authority::DevTrust, next).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("dev sign-in refused: {e}");
            Redirect::to(&LoginError::from(&e).url(next)).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dev_form_keeps_next_and_links_back_to_the_chooser() {
        let html = form_page("/v/a?b=1&c=\"2\"");
        assert!(html.contains(r#"name="next" value="/v/a?b=1&amp;c=&quot;2&quot;""#));
        assert!(html.contains("Other sign-in options"));
        assert!(html.contains(r#"href="/auth/login?next=%2Fv%2Fa%3Fb%3D1%26c%3D%222%22""#));
    }
}
