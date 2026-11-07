//! `/auth/dev/login` — trust-mode login. GET renders the email form
//! (reached from the [`super::login`] chooser), POST mints the session
//! for whatever email was typed and returns to `next`. Everyone is
//! admin. Exists only in `--features dev-login` builds and only when
//! `ZIM_HUB_AUTH` lists `dev-trust`; the router is not even mounted
//! otherwise.

use askama::Template;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Form, Router};
use serde::Deserialize;

use crate::http::auth::login::LoginError;
use crate::http::auth::session::{sign_in, Authority, Destination};
use crate::http::auth::{mobile, return_to};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/login", get(form).post(submit))
}

#[derive(Deserialize)]
struct FormQuery {
    next: Option<String>,
    /// From the native app; carried through the form (see [`super::mobile`]).
    mobile_challenge: Option<String>,
}

async fn form(Query(query): Query<FormQuery>) -> Response {
    let next = return_to::sanitize(query.next.as_deref());
    let challenge = query.mobile_challenge.as_deref();
    if challenge.is_some_and(|c| !mobile::valid_challenge(c)) {
        return (StatusCode::BAD_REQUEST, "malformed mobile_challenge").into_response();
    }
    form_page(next, challenge).into_response()
}

/// `templates/auth/dev_login.html`: the email form.
#[derive(Template)]
#[template(path = "auth/dev_login.html")]
struct DevLoginPage<'a> {
    heading: &'static str,
    next: &'a str,
    next_encoded: String,
    mobile_challenge: Option<&'a str>,
}

fn form_page<'a>(next: &'a str, mobile_challenge: Option<&'a str>) -> DevLoginPage<'a> {
    DevLoginPage {
        heading: "Dev login",
        next,
        mobile_challenge,
        next_encoded: urlencoding::encode(next).into_owned(),
    }
}

#[derive(Deserialize)]
struct Submit {
    email: String,
    #[serde(default)]
    next: Option<String>,
    #[serde(default)]
    mobile_challenge: Option<String>,
}

async fn submit(State(state): State<AppState>, Form(f): Form<Submit>) -> Response {
    let email = f.email.trim().to_lowercase();
    if email.is_empty() {
        return (StatusCode::BAD_REQUEST, "email required").into_response();
    }
    let name = email.split('@').next().unwrap_or("dev").to_string();
    let next = return_to::sanitize(f.next.as_deref());
    let dest = match f.mobile_challenge.as_deref() {
        Some(challenge) if mobile::valid_challenge(challenge) => Destination::App { challenge },
        Some(_) => return (StatusCode::BAD_REQUEST, "malformed mobile_challenge").into_response(),
        None => Destination::Web { next },
    };
    match sign_in(&state, &email, &name, Authority::DevTrust, dest).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("dev sign-in refused: {e}");
            dest.failure(LoginError::from(&e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dev_form_keeps_next_and_links_back_to_the_chooser() {
        let html = form_page("/v/a?b=1&c=\"2\"", None).render().unwrap();
        // The quote is escaped, so `next` can't break out of its attribute.
        assert!(html.contains(r#"name="next" value="/v/a?b=1&amp;c=&quot;2&quot;""#));
        assert!(html.contains("Other sign-in options"));
        assert!(html.contains(r#"href="/auth/login?next=%2Fv%2Fa%3Fb%3D1%26c%3D%222%22""#));
    }

    #[test]
    fn alices_phone_carries_its_challenge_through_the_dev_form() {
        let html = form_page("/", Some("abc_-123")).render().unwrap();
        assert!(html.contains(r#"name="mobile_challenge" value="abc_-123""#));
        let web = form_page("/", None).render().unwrap();
        assert!(!web.contains("mobile_challenge"));
    }
}
