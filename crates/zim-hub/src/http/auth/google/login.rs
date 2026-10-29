//! `GET /auth/google/login?next=…` — kick off the OAuth handshake by
//! redirecting to Google's consent screen. `next` rides in the OAuth
//! `state` param, bound to this browser by a cookie (see [`super::state`]).

use axum::extract::{Query, State};
use axum::http::header::SET_COOKIE;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

use super::state as oauth_state;
use crate::http::auth::mobile;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LoginQuery {
    next: Option<String>,
    /// From the native app: finish by handing a token back to it (see
    /// [`crate::http::auth::mobile`]) instead of setting a cookie here.
    mobile_challenge: Option<String>,
}

pub async fn handler(State(state): State<AppState>, Query(query): Query<LoginQuery>) -> Response {
    // Only mounted when Google is an enabled provider.
    let Some(google) = state.auth.google() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let client_id = &google.client_id;
    let redirect_uri = state.auth.callback_url();
    let secure = state.auth.host_name.starts_with("https://");
    let challenge = query.mobile_challenge.as_deref();
    if challenge.is_some_and(|c| !mobile::valid_challenge(c)) {
        return (StatusCode::BAD_REQUEST, "malformed mobile_challenge").into_response();
    }
    let (oauth_state, cookies) =
        oauth_state::begin(query.next.as_deref().unwrap_or("/"), challenge, secure);

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
         access_type=online&\
         state={state}",
        client_id = urlencoding::encode(client_id),
        redirect = urlencoding::encode(&redirect_uri),
        state = urlencoding::encode(&oauth_state),
    );

    let mut res = Redirect::temporary(&url).into_response();
    for cookie in cookies.iter().filter_map(|c| c.parse().ok()) {
        res.headers_mut().append(SET_COOKIE, cookie);
    }
    res
}
