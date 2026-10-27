//! `POST /auth/logout` — clear the session cookie and bounce to
//! `/`. `POST` (not `GET`) because logout has side effects and `GET`
//! makes it CSRF-trivial.

use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::response::{IntoResponse, Redirect, Response};

use crate::state::AppState;

pub async fn handler(State(state): State<AppState>) -> Response {
    let name = state.auth.session_cookie();
    let cookie = format!("{name}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    ([(SET_COOKIE, cookie)], Redirect::to("/")).into_response()
}
