//! `GET /auth/login` — the sign-in page. Server-rendered and separate
//! from the wasm app (which the hub only serves with a session): one
//! entry per enabled provider, each carrying `?next=` through to where
//! the visitor was headed. Markup mirrors the SPA's `OnboardingShell`
//! (`.onb`, `.gate`, `.field`, `.btn`) so both share `style.css`.

use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;

use crate::config::AuthProvider;
use crate::http::auth::{return_to, OptionalUser};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LoginQuery {
    next: Option<String>,
}

pub async fn handler(
    State(state): State<AppState>,
    OptionalUser(user): OptionalUser,
    Query(query): Query<LoginQuery>,
) -> Response {
    let next = return_to::sanitize(query.next.as_deref());
    // Already signed in: nothing to choose.
    if user.is_some() {
        return Redirect::to(next).into_response();
    }
    Html(page(&state.auth.providers, next)).into_response()
}

fn page(providers: &[AuthProvider], next: &str) -> String {
    let options = providers
        .iter()
        .map(|p| option(p, next))
        .collect::<Vec<_>>()
        .join(r#"<p class="dim" style="text-align:center;margin:1rem 0;">or</p>"#);
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>sign in — zim</title>
  <link rel="icon" type="image/svg+xml" href="/static/favicon.svg">
  <link rel="stylesheet" href="/static/style.css">
</head>
<body>
  <div class="onb">
    <header class="onb__bar"><span class="onb__brand">zim</span></header>
    <main class="onb__main"><div class="onb__inner">
      <div class="gate" style="max-width:440px;margin:0 auto;">
        <h3 style="margin-top:0;">Sign in</h3>
        {options}
      </div>
    </div></main>
  </div>
</body>
</html>"#
    )
}

fn option(provider: &AuthProvider, next: &str) -> String {
    match provider {
        AuthProvider::Google(_) => format!(
            r#"<a class="btn btn-primary" href="/auth/google/login?next={next}" style="width:100%;text-align:center;">Sign in with Google</a>"#,
            next = urlencoding::encode(next),
        ),
        #[cfg(feature = "dev-login")]
        AuthProvider::DevTrust => format!(
            r#"<form method="post" action="/auth/dev/login">
          <div class="error" style="margin-bottom:0.75rem;">Dev trust mode — any email signs in as admin.</div>
          <input type="hidden" name="next" value="{next}">
          <div class="field">
            <label for="email">Email</label>
            <input id="email" type="email" name="email" required autofocus>
          </div>
          <button type="submit" class="btn btn-primary" style="width:100%;">Dev login</button>
        </form>"#,
            next = escape_attr(next),
        ),
    }
}

#[cfg(feature = "dev-login")]
fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GoogleOAuth;

    fn google() -> AuthProvider {
        AuthProvider::Google(GoogleOAuth {
            client_id: "id".into(),
            client_secret: "secret".into(),
        })
    }

    #[test]
    fn google_button_carries_next_through_the_handshake() {
        let html = page(&[google()], "/device?code=XYZ");
        assert!(html.contains(r#"href="/auth/google/login?next=%2Fdevice%3Fcode%3DXYZ""#));
        assert!(!html.contains("Dev trust mode"));
    }

    #[cfg(feature = "dev-login")]
    #[test]
    fn dev_hub_offers_both_and_the_dev_form_keeps_next() {
        let html = page(&[AuthProvider::DevTrust, google()], "/v/a?b=1&c=\"2\"");
        let dev = html.find("Dev trust mode").unwrap();
        let google = html.find("Sign in with Google").unwrap();
        assert!(dev < google, "providers render in configured order");
        assert!(html.contains(r#"name="next" value="/v/a?b=1&amp;c=&quot;2&quot;""#));
    }
}
