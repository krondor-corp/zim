//! `GET /auth/login` — choose how to sign in. Server-rendered and
//! separate from the wasm app (which the hub only serves with a
//! session): one button per enabled provider, always — even when there
//! is only one — each carrying `?next=` to where the visitor was headed.
//! Providers with their own step (dev trust's email form) render it on
//! their own page through [`shell`]. Markup mirrors the SPA's
//! `OnboardingShell` (`.onb`, `.gate`, `.btn`) so both share `style.css`.

use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;

use crate::config::AuthProvider;
use crate::http::auth::session::SessionError;
use crate::http::auth::{return_to, OptionalUser};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LoginQuery {
    next: Option<String>,
    error: Option<String>,
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
    let error = query.error.as_deref().and_then(LoginError::from_code);
    Html(chooser(&state.auth.providers, next, error)).into_response()
}

/// Why the visitor is back at the chooser. Carried as a fixed code, so
/// nothing from the URL is ever echoed into the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoginError {
    /// The email's account belongs to another provider (no linking).
    WrongProvider,
    /// The email's account belongs to a different account at the same
    /// provider (Google `sub` mismatch).
    AccountMismatch,
    /// Anything else (handshake failed, unverified email, …).
    Failed,
}

impl LoginError {
    fn code(self) -> &'static str {
        match self {
            LoginError::WrongProvider => "wrong-provider",
            LoginError::AccountMismatch => "account-mismatch",
            LoginError::Failed => "failed",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        [Self::WrongProvider, Self::AccountMismatch, Self::Failed]
            .into_iter()
            .find(|e| e.code() == code)
    }

    fn message(self) -> &'static str {
        match self {
            LoginError::WrongProvider => {
                "That email's account uses a different sign-in method. Accounts aren't linked across sign-in methods — use the one you signed up with."
            }
            LoginError::AccountMismatch => {
                "That email belongs to a different account at this provider."
            }
            LoginError::Failed => "Sign-in didn't complete. Try again.",
        }
    }

    /// The chooser, showing this error, returning to `next` afterwards.
    pub fn url(self, next: &str) -> String {
        let base = return_to::login_url(next);
        let sep = if base.contains('?') { '&' } else { '?' };
        format!("{base}{sep}error={}", self.code())
    }
}

impl From<&SessionError> for LoginError {
    fn from(e: &SessionError) -> Self {
        match e {
            SessionError::WrongProvider { .. } => LoginError::WrongProvider,
            SessionError::SubjectMismatch { .. } => LoginError::AccountMismatch,
            SessionError::User(_) | SessionError::Token(_) => LoginError::Failed,
        }
    }
}

fn chooser(providers: &[AuthProvider], next: &str, error: Option<LoginError>) -> String {
    let banner = error
        .map(|e| {
            format!(
                r#"<div class="error" style="margin-bottom:0.75rem;">{}</div>"#,
                e.message()
            )
        })
        .unwrap_or_default();
    let next = urlencoding::encode(next);
    let buttons: String = providers
        .iter()
        .map(|p| {
            let (path, label) = entry(p);
            format!(
                r#"<a class="btn btn-primary" href="{path}?next={next}" style="width:100%;text-align:center;margin-bottom:0.5rem;">{label}</a>"#
            )
        })
        .collect();
    shell("Sign in", &format!("{banner}{buttons}"))
}

/// Where a provider's sign-in starts, and its button label.
fn entry(provider: &AuthProvider) -> (&'static str, &'static str) {
    match provider {
        AuthProvider::Google(_) => ("/auth/google/login", "Sign in with Google"),
        #[cfg(feature = "dev-login")]
        AuthProvider::DevTrust => ("/auth/dev/login", "Dev login"),
    }
}

/// Page frame shared by every `/auth/*` page: the SPA's onboarding look.
pub(super) fn shell(heading: &str, body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{heading} — zim</title>
  <link rel="icon" type="image/svg+xml" href="/static/favicon.svg">
  <link rel="stylesheet" href="/static/style.css">
</head>
<body>
  <div class="onb">
    <header class="onb__bar"><span class="onb__brand">zim</span></header>
    <main class="onb__main"><div class="onb__inner">
      <div class="gate" style="max-width:440px;margin:0 auto;">
        <h3 style="margin-top:0;">{heading}</h3>
        {body}
      </div>
    </div></main>
  </div>
</body>
</html>"#
    )
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
    fn a_single_provider_still_gets_a_chooser() {
        let html = chooser(&[google()], "/device?code=XYZ", None);
        assert!(html.contains(r#"href="/auth/google/login?next=%2Fdevice%3Fcode%3DXYZ""#));
        assert!(!html.contains(r#"class="error""#));
    }

    #[cfg(feature = "dev-login")]
    #[test]
    fn dev_hub_lists_every_provider_in_configured_order() {
        let html = chooser(&[AuthProvider::DevTrust, google()], "/", None);
        let dev = html.find(r#"href="/auth/dev/login?next=%2F""#).unwrap();
        let google = html.find(r#"href="/auth/google/login?next=%2F""#).unwrap();
        assert!(dev < google);
    }

    #[test]
    fn bounced_sign_ins_explain_themselves_by_fixed_code_only() {
        let html = chooser(&[google()], "/", Some(LoginError::WrongProvider));
        assert!(html.contains("aren't linked"));
        assert_eq!(LoginError::from_code("<script>"), None);
    }

    #[test]
    fn error_urls_keep_next() {
        assert_eq!(LoginError::Failed.url("/"), "/auth/login?error=failed");
        assert_eq!(
            LoginError::WrongProvider.url("/v/a"),
            "/auth/login?next=%2Fv%2Fa&error=wrong-provider"
        );
    }
}
