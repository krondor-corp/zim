//! `GET /api/v0/vaults` — vaults the signed-in user owns (mirrored here).
//!
//! JSON form of the server-rendered workspace index: walk the relay's
//! vaults, keep the ones whose head-manifest shareholders include one of the
//! user's peers (admins see all), return id + name. The SPA renders the list.

use axum::extract::State;
use axum::Json;
// Shared wire types — mirrored by `zim_api::hub::VaultsRequest`.
use zim_api::hub::{VaultItem, VaultsResponse};

use crate::access::{can_access_vault_via_db, read_manifest_meta};
use crate::http::auth::RequireOnboardedUser;
use crate::state::AppState;

pub async fn handler(
    State(state): State<AppState>,
    RequireOnboardedUser(user): RequireOnboardedUser,
) -> Json<VaultsResponse> {
    let listings = state.peer.list_vaults().await.unwrap_or_else(|e| {
        tracing::warn!("list_vaults failed: {e}");
        Vec::new()
    });

    let mut vaults = Vec::new();
    for v in listings {
        let Some(meta) = read_manifest_meta(&state, v.id).await else {
            continue;
        };
        let visible =
            user.is_admin() || can_access_vault_via_db(&state.db, &user, &meta.shareholders).await;
        if !visible {
            continue;
        }
        vaults.push(VaultItem {
            vault_id: v.id.to_string(),
            name: meta.name,
        });
    }
    Json(VaultsResponse { vaults })
}
