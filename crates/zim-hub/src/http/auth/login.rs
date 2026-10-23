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
    /// With `error=wrong-provider`: the provider the account uses…
    used: Option<String>,
    /// …and the one that was tried.
    tried: Option<String>,
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
    let error = query
        .error
        .as_deref()
        .map(|code| LoginError::from_query(code, query.used.as_deref(), query.tried.as_deref()));
    Html(chooser(&state.auth.providers, next, error)).into_response()
}

/// Why the visitor is back at the chooser. Carried as a fixed code plus
/// provider names; only known values map to text, so nothing from the
/// URL is ever echoed into the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LoginError {
    /// The email's account belongs to another provider (no linking).
    /// Both are provider names (`google`, `dev-trust`).
    WrongProvider {
        used: &'static str,
        tried: &'static str,
    },
    /// The email belongs to a different Google account (`sub` mismatch).
    AccountMismatch,
    /// Anything else (handshake failed, unverified email, …).
    Failed,
}

/// Provider names (`ZIM_HUB_AUTH` / `users.auth_provider`) and how the
/// error copy refers to them.
const PROVIDER_LABELS: [(&str, &str); 2] = [("google", "Google"), ("dev-trust", "Dev login")];

fn known_provider(name: &str) -> Option<&'static str> {
    PROVIDER_LABELS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(n, _)| *n)
}

fn provider_label(name: &str) -> &'static str {
    PROVIDER_LABELS
        .iter()
        .find(|(n, _)| *n == name)
        .map_or("another method", |(_, label)| *label)
}

impl LoginError {
    fn from_query(code: &str, used: Option<&str>, tried: Option<&str>) -> Self {
        match code {
            "wrong-provider" => match (
                used.and_then(known_provider),
                tried.and_then(known_provider),
            ) {
                (Some(used), Some(tried)) => LoginError::WrongProvider { used, tried },
                _ => LoginError::Failed,
            },
            "account-mismatch" => LoginError::AccountMismatch,
            _ => LoginError::Failed,
        }
    }

    fn message(self) -> String {
        match self {
            LoginError::WrongProvider { used, tried } => format!(
                "You signed up with {}. Use that — you can't sign in with {}.",
                provider_label(used),
                provider_label(tried),
            ),
            LoginError::AccountMismatch => {
                "This email belongs to a different Google account.".to_string()
            }
            LoginError::Failed => "Sign-in failed. Try again.".to_string(),
        }
    }

    fn query(self) -> String {
        match self {
            LoginError::WrongProvider { used, tried } => {
                format!("error=wrong-provider&used={used}&tried={tried}")
            }
            LoginError::AccountMismatch => "error=account-mismatch".to_string(),
            LoginError::Failed => "error=failed".to_string(),
        }
    }

    /// The chooser, showing this error, returning to `next` afterwards.
    pub fn url(self, next: &str) -> String {
        let base = return_to::login_url(next);
        let sep = if base.contains('?') { '&' } else { '?' };
        format!("{base}{sep}{}", self.query())
    }
}

impl From<&SessionError> for LoginError {
    fn from(e: &SessionError) -> Self {
        match e {
            SessionError::WrongProvider {
                provider,
                attempted,
                ..
            } => match known_provider(provider) {
                Some(used) => LoginError::WrongProvider {
                    used,
                    tried: attempted,
                },
                None => LoginError::Failed,
            },
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

    const DEV_INTO_GOOGLE: LoginError = LoginError::WrongProvider {
        used: "google",
        tried: "dev-trust",
    };

    #[test]
    fn a_wrong_provider_bounce_says_which_method_to_use() {
        let html = chooser(&[google()], "/", Some(DEV_INTO_GOOGLE));
        assert!(html
            .contains("You signed up with Google. Use that — you can't sign in with Dev login."));
    }

    #[test]
    fn the_error_round_trips_through_the_url() {
        let url = DEV_INTO_GOOGLE.url("/v/a");
        assert_eq!(
            url,
            "/auth/login?next=%2Fv%2Fa&error=wrong-provider&used=google&tried=dev-trust"
        );
        assert_eq!(
            LoginError::from_query("wrong-provider", Some("google"), Some("dev-trust")),
            DEV_INTO_GOOGLE
        );
        assert_eq!(LoginError::Failed.url("/"), "/auth/login?error=failed");
    }

    #[test]
    fn nothing_from_the_url_reaches_the_page() {
        let forged = LoginError::from_query("wrong-provider", Some("<script>"), Some("google"));
        assert_eq!(forged, LoginError::Failed);
        let html = chooser(&[google()], "/", Some(forged));
        assert!(!html.contains("<script>"));
        assert_eq!(
            LoginError::from_query("<b>", None, None),
            LoginError::Failed
        );
    }
}
