//! `GET /api/v0/me` — the signed-in user's id, hub host, and email.
//!
//! The browser SPA fetches this to build did fragments
//! (`did:web:<host>:u:<user_id>#…`) for web-key creation and to show who's
//! signed in. `RequireUser` (not onboarded) so a keyless user can still
//! reach it to *create* their web key.

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::database::models::UserPeer;
use crate::http::auth::RequireUser;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub user_id: String,
    pub host: String,
    pub email: String,
    /// This account's `did:web:<host>:u:<user_id>` — the identity the web/
    /// browser key *is*. It's reached via the hub (not by dialing the
    /// browser), so it's the correct thing to show/share for the web key
    /// rather than its raw `did:key`. Resolves to `/u/<user_id>/did.json`.
    pub did: String,
    /// Whether this account has a browser/web key enrolled. The SPA's
    /// web-key gate uses this to choose *create* (onboard) vs *unlock*,
    /// and to ignore a stale tab-cached seed after a hub reset.
    pub has_web_key: bool,
}

pub async fn handler(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
) -> Json<MeResponse> {
    let has_web_key = UserPeer::user_has_web_key(user.id(), &state.db)
        .await
        .unwrap_or(false);
    // `state.did` is `did:web:<host>`; the account DID appends `:u:<user_id>`.
    let did = format!("{}:u:{}", state.did, user.id());
    Json(MeResponse {
        user_id: user.id().to_string(),
        host: state.host.clone(),
        email: user.email().to_string(),
        did,
        has_web_key,
    })
}
