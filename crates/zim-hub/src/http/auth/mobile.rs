//! Sign-in for the native app (the iOS shell), whose webview can't run
//! Google OAuth itself (Google refuses embedded webviews). The app hands
//! the sign-in to a system web-auth session and gets the result back
//! through a custom URL scheme:
//!
//! 1. App → system browser: the provider's usual start URL plus
//!    `mobile_challenge=<base64url SHA-256 of a verifier the app keeps>`.
//! 2. Provider handshake runs unchanged; on success the hub issues a
//!    one-time token (see [`MobileHandoff`]) and redirects to
//!    `zim-auth://callback?token=…` — no cookie in the system browser.
//! 3. App → its own webview: `GET /auth/mobile/complete?token=…&verifier=…`
//!    ([`complete`]) sets the ordinary session cookie there and lands on
//!    the SPA, which carries on to the passphrase unlock exactly like a
//!    browser tab.
//!
//! Another app can register `zim-auth://` too and read the token off the
//! callback. Without the verifier (which never leaves the app) the token
//! is worthless, and it dies on first use either way.

use axum::extract::{Query, State};
use axum::http::header::SET_COOKIE;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use super::login::LoginError;
use super::session::session_cookie;
use crate::config::AuthConfig;
use crate::database::models::mobile_handoff::Refused;
use crate::database::models::{MobileHandoff, User};
use crate::database::Database;
use crate::state::AppState;

/// The app's callback scheme, registered in its `Info.plist`.
pub const CALLBACK_SCHEME: &str = "zim-auth";

/// Length of a base64url (unpadded) SHA-256 digest.
const CHALLENGE_LEN: usize = 43;

/// The challenge from `mobile_challenge`, if it is well-formed: exactly
/// a base64url SHA-256, so it is safe to carry in a cookie and a column.
pub fn valid_challenge(challenge: &str) -> bool {
    challenge.len() == CHALLENGE_LEN
        && challenge
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Hand the app its one-time token.
pub fn callback(token: &str) -> Response {
    redirect_to_app(&format!("{CALLBACK_SCHEME}://callback?token={token}"))
}

/// Tell the app the sign-in didn't complete (it reloads the hub).
pub fn failed() -> Response {
    redirect_to_app(&format!("{CALLBACK_SCHEME}://callback?error=failed"))
}

fn redirect_to_app(url: &str) -> Response {
    Redirect::temporary(url).into_response()
}

#[derive(Deserialize)]
pub struct CompleteQuery {
    token: String,
    verifier: String,
}

/// `GET /auth/mobile/complete?token=…&verifier=…` — redeem a handoff in
/// the app's webview. Every refusal looks the same: back to the sign-in
/// chooser.
pub async fn complete(State(state): State<AppState>, Query(q): Query<CompleteQuery>) -> Response {
    match redeem(&state.db, &state.auth, &q.token, &q.verifier).await {
        Some(cookie) => ([(SET_COOKIE, cookie)], Redirect::to("/")).into_response(),
        None => Redirect::to(&LoginError::Failed.url("/")).into_response(),
    }
}

/// The `Set-Cookie` that signs the app in, if `token` + `verifier` check out.
async fn redeem(db: &Database, auth: &AuthConfig, token: &str, verifier: &str) -> Option<String> {
    let user_id = match MobileHandoff::consume(token, verifier, db).await {
        Ok(Ok(id)) => id,
        Ok(Err(Refused::NoSuchToken)) => {
            tracing::warn!("mobile handoff: unknown, used or expired token");
            return None;
        }
        Ok(Err(Refused::WrongVerifier)) => {
            tracing::warn!("mobile handoff: verifier mismatch");
            return None;
        }
        Err(e) => {
            tracing::error!("mobile handoff lookup failed: {e}");
            return None;
        }
    };
    let user = User::find_by_id(user_id, db).await.ok().flatten()?;
    session_cookie(auth, &user)
        .map_err(|e| tracing::error!("mobile session encode failed: {e}"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::models::mobile_handoff::challenge_for;
    use crate::test_fixtures::{auth_config, user};
    use axum::http::header::LOCATION;
    use chrono::Duration;

    /// What the app keeps (the verifier) and what it sends to the hub.
    const VERIFIER: &str = "a-long-random-string-only-alices-phone-knows-0123456789";

    fn token_in(callback: &Response) -> String {
        let location = callback.headers()[LOCATION].to_str().unwrap();
        location
            .strip_prefix("zim-auth://callback?token=")
            .expect("callback carries a token")
            .to_string()
    }

    #[tokio::test]
    async fn alice_signs_in_on_her_phone_and_lands_in_the_unlock_screen() {
        let db = Database::in_memory().await;
        let auth = auth_config(&[]);
        let alice = user(&db, "alice").await;

        // The system browser finished Google; the hub hands the app a token…
        let token = MobileHandoff::issue(alice.id(), &challenge_for(VERIFIER), &db)
            .await
            .unwrap();
        let callback = callback(&token);
        assert_eq!(token_in(&callback), token);
        // …and no session cookie rides along in the system browser.
        assert!(callback.headers().get(SET_COOKIE).is_none());

        // The app redeems it in its webview: a normal session cookie for Alice.
        let cookie = redeem(&db, &auth, &token, VERIFIER).await.unwrap();
        assert!(cookie.starts_with(&format!("{}=", auth.session_cookie())));
        assert!(cookie.contains("HttpOnly"));
        let jwt = cookie.split(';').next().unwrap().split_once('=').unwrap().1;
        let claims = jsonwebtoken::decode::<super::super::Claims>(
            jwt,
            &jsonwebtoken::DecodingKey::from_secret(auth.service_secret.as_bytes()),
            &jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256),
        )
        .unwrap()
        .claims;
        assert_eq!(claims.email, "alice@example.com");
    }

    #[tokio::test]
    async fn a_handoff_token_works_once() {
        let db = Database::in_memory().await;
        let auth = auth_config(&[]);
        let alice = user(&db, "alice").await;
        let token = MobileHandoff::issue(alice.id(), &challenge_for(VERIFIER), &db)
            .await
            .unwrap();

        assert!(redeem(&db, &auth, &token, VERIFIER).await.is_some());
        // Replaying the very same URL, verifier and all, gets nothing.
        assert!(redeem(&db, &auth, &token, VERIFIER).await.is_none());
    }

    #[tokio::test]
    async fn a_hijacked_callback_without_the_verifier_is_refused() {
        let db = Database::in_memory().await;
        let auth = auth_config(&[]);
        let alice = user(&db, "alice").await;
        let token = MobileHandoff::issue(alice.id(), &challenge_for(VERIFIER), &db)
            .await
            .unwrap();

        // Mallory's app also claims `zim-auth://` and reads the token off
        // the callback, but never saw the verifier.
        assert!(redeem(&db, &auth, &token, "mallorys-guess").await.is_none());
        assert!(redeem(&db, &auth, &token, "").await.is_none());
        // Her try burned it: Alice's real app can't redeem it either (it
        // restarts sign-in), and neither can Mallory with a better guess.
        assert!(redeem(&db, &auth, &token, VERIFIER).await.is_none());
    }

    #[tokio::test]
    async fn expired_handoff_token_is_refused() {
        let db = Database::in_memory().await;
        let auth = auth_config(&[]);
        let alice = user(&db, "alice").await;
        let stale = MobileHandoff::issue_for(
            alice.id(),
            &challenge_for(VERIFIER),
            Duration::seconds(-1),
            &db,
        )
        .await
        .unwrap();

        assert!(redeem(&db, &auth, &stale, VERIFIER).await.is_none());
    }

    #[tokio::test]
    async fn a_token_is_stored_only_as_a_hash() {
        let db = Database::in_memory().await;
        let alice = user(&db, "alice").await;
        let token = MobileHandoff::issue(alice.id(), &challenge_for(VERIFIER), &db)
            .await
            .unwrap();

        let stored: Vec<(String,)> = sqlx::query_as("SELECT token_hash FROM mobile_handoffs")
            .fetch_all(&*db)
            .await
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_ne!(stored[0].0, token);
    }

    #[test]
    fn only_a_sha256_challenge_is_accepted_at_sign_in_start() {
        assert!(valid_challenge(&challenge_for(VERIFIER)));
        for bad in [
            "",
            "short",
            &"a".repeat(44),
            &format!("{};x", "a".repeat(42)),
        ] {
            assert!(!valid_challenge(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_failed_sign_in_tells_the_app_instead_of_stranding_the_sheet() {
        let res = failed();
        assert_eq!(res.headers()[LOCATION], "zim-auth://callback?error=failed");
    }
}
