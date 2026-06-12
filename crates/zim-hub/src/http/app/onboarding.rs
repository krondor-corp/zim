//! `/app/onboarding` — forced first-time setup.
//!
//! Reached automatically when [`RequireOnboardedUser`](crate::http::auth::RequireOnboardedUser)
//! finds zero rows in `user_peers` for the requesting user. Anonymous
//! requests fall through to `/auth/google/login`; authorized-but-
//! pending users fall through to `/auth/pending`. Authenticated +
//! authorized + zero devices land here.
//!
//! The page itself is two clear options:
//! 1. Register a daemon (already paste-pubkey form).
//! 2. Set up this browser (placeholder — needs WebCrypto wiring).
//!
//! As soon as the user lands their first device row, every other
//! `/app/*` page becomes reachable.

use askama::Template;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Redirect, Response};
use uuid::Uuid;

use crate::database::models::UserPeer;
use crate::errors::Result;
use crate::http::auth::RequireUser;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "pages/onboarding.html")]
struct OnboardingTemplate {
    hub_version: &'static str,
    user_email: String,
    is_admin: bool,
    active_nav: &'static str,
    user_id: Uuid,
}

/// `GET /app/onboarding`. If the user has already added a device,
/// short-circuit to the workspace so they don't re-onboard.
pub async fn handler(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
) -> Result<Response> {
    if UserPeer::user_has_web_key(user.id(), &state.db)
        .await
        .unwrap_or(false)
    {
        return Ok(Redirect::to("/app").into_response());
    }
    let tmpl = OnboardingTemplate {
        hub_version: env!("CARGO_PKG_VERSION"),
        user_email: user.email().to_string(),
        is_admin: user.is_admin(),
        active_nav: "",
        user_id: user.id(),
    };
    Ok(Html(tmpl.render()?).into_response())
}
