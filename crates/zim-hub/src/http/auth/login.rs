//! `GET /auth/login` — the provider-neutral sign-in entry. One enabled
//! provider: redirect straight to it. Several: render a chooser.

use axum::extract::State;
use axum::response::{Html, IntoResponse, Redirect, Response};

use crate::config::AuthProvider;
use crate::state::AppState;

pub async fn handler(State(state): State<AppState>) -> Response {
    match state.auth.providers.as_slice() {
        [only] => Redirect::to(entry(only).path).into_response(),
        providers => Html(chooser(providers)).into_response(),
    }
}

/// How the chooser presents a provider, and where it starts.
struct Entry {
    path: &'static str,
    label: &'static str,
    /// Shown under the button — for providers an operator should think
    /// twice about.
    warning: Option<&'static str>,
}

fn entry(provider: &AuthProvider) -> Entry {
    match provider {
        AuthProvider::Google(_) => Entry {
            path: "/auth/google/login",
            label: "Sign in with Google",
            warning: None,
        },
        #[cfg(feature = "dev-login")]
        AuthProvider::DevTrust => Entry {
            path: "/auth/dev/login",
            label: "Dev login",
            warning: Some("any email signs in as admin"),
        },
    }
}

fn chooser(providers: &[AuthProvider]) -> String {
    let options: String = providers
        .iter()
        .map(entry)
        .map(|e| {
            let warning = e
                .warning
                .map(|w| format!(r#"<br><small class="muted">{w}</small>"#))
                .unwrap_or_default();
            format!(
                r#"<li><a class="button" href="{}">{}</a>{warning}</li>"#,
                e.path, e.label
            )
        })
        .collect();
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>sign in — zim-hub</title>
  <link rel="stylesheet" href="/static/style.css">
</head>
<body>
  <header><h1>zim-hub</h1></header>
  <main>
    <h2>sign in</h2>
    <ul style="list-style:none;padding:0">{options}</ul>
  </main>
</body>
</html>"#
    )
}
