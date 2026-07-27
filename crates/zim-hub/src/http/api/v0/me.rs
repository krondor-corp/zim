//! `GET /api/v0/me` — the signed-in user's id, hub host, and email.
//!
//! The browser SPA fetches this to build did fragments
//! (`did:web:<host>:u:<user_id>#…`) for web-key creation and to show who's
//! signed in. `RequireUser` (not onboarded) so a keyless user can still
//! reach it to *create* their web key.

use axum::extract::State;
use axum::Json;
// Shared wire type — the same `Me` the web SPA (and the typed client in
// `zim_api::hub`) deserializes. Field semantics are documented there.
use zim_api::hub::Me;

use crate::database::models::UserPeer;
use crate::http::auth::RequireUser;
use crate::state::AppState;

pub async fn handler(State(state): State<AppState>, RequireUser(user): RequireUser) -> Json<Me> {
    let has_web_key = UserPeer::user_has_web_key(user.id(), &state.db)
        .await
        .unwrap_or(false);
    // `state.did` is `did:web:<host>`; the account DID appends `:u:<user_id>`.
    let did = format!("{}:u:{}", state.did, user.id());
    Json(Me {
        user_id: user.id().to_string(),
        host: state.host.clone(),
        email: user.email().to_string(),
        did,
        has_web_key,
    })
}
