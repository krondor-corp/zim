//! `GET /api/v0/v/{vault_id}/head` — current canonical head + height.
//!
//! Gated by [`crate::access::can_access_vault`]: non-owners get 404.
//! Reads from `coord.log()` directly — the hub holds ciphertext + the
//! log, but not a Share, so the higher-level `Peer::vault()` would
//! fail with `ShareNotFound`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use zim_core::linked_data::Link;
use zim_core::vault::VaultId;
use zim_peer::VaultLog;

use crate::access::can_access_vault;
use crate::http::auth::RequireUser;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct HeadResponse {
    pub link: Link,
    pub height: u64,
}

pub async fn handler(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
    Path(vault_id): Path<VaultId>,
) -> Response {
    if !can_access_vault(&state, &user, vault_id).await {
        return (StatusCode::NOT_FOUND, "vault not found").into_response();
    }
    let log = state.service.peer().coord().log();
    match log.head(vault_id, None).await {
        Ok(head) => (
            StatusCode::OK,
            Json(HeadResponse {
                link: head.link,
                height: head.height,
            }),
        )
            .into_response(),
        Err(e) => {
            tracing::warn!("vault {vault_id} head: {e}");
            (StatusCode::NOT_FOUND, "vault not found").into_response()
        }
    }
}
