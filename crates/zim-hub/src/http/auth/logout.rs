//! `POST /auth/logout` — clear the session cookie and bounce to
//! `/`. `POST` (not `GET`) because logout has side effects and `GET`
//! makes it CSRF-trivial.

use axum::http::header::SET_COOKIE;
use axum::response::{IntoResponse, Redirect, Response};

use crate::http::auth::SESSION_COOKIE;

pub async fn handler() -> Response {
    let cookie = format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    ([(SET_COOKIE, cookie)], Redirect::to("/")).into_response()
}
