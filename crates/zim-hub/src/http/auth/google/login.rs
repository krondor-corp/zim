//! `GET /auth/google/login` — kick off the OAuth handshake by
//! redirecting to Google's consent screen.
//!
//! No CSRF `state` parameter today: the callback's DB-side
//! find_or_create + role check is the actual authorization gate. A
//! forged callback at worst registers an attacker as a pending user,
//! which an admin then doesn't approve. Adding `state` becomes
//! load-bearing as soon as we add a "link a second identity" flow.

use axum::extract::State;
use axum::response::{IntoResponse, Redirect, Response};

use crate::state::AppState;

pub async fn handler(State(state): State<AppState>) -> Response {
    let client_id = &state.auth.google_o_auth_client_id;
    let redirect_uri = state.auth.callback_url();

    // Scopes: `openid email profile`. `openid` + `email` carry the
    // claims we need; `profile` adds the display name we put in the
    // session + the `users` row. `access_type=online` — we never
    // refresh, so don't ask for a refresh token.
    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?\
         client_id={client_id}&\
         redirect_uri={redirect}&\
         response_type=code&\
         scope=openid%20email%20profile&\
         access_type=online",
        client_id = urlencoding::encode(client_id),
        redirect = urlencoding::encode(&redirect_uri),
    );

    Redirect::temporary(&url).into_response()
}
