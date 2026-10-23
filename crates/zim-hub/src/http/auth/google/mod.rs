//! `/auth/google/*` — OAuth handshake with Google's standard web
//! flow. No PKCE (Google's confidential-client web flow uses
//! client_secret instead); the redirect URI must be registered in the
//! OAuth client config exactly.

pub mod callback;
pub mod login;
mod state;

use axum::routing::get;
use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", get(login::handler))
        .route("/callback", get(callback::handler))
}
