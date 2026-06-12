//! `GET /api/v0/v/{vault_id}/log?from=N&limit=M` — paginated chain
//! walk, newest first.
//!
//! Walks heights `from` down to `max(0, from-limit)`. Each entry is
//! `(height, link)` where `link` is the canonical (lexicographically
//! greatest) head at that height. The browser uses this to drive a
//! history view; each link points to a ciphertext Manifest blob it
//! fetches via the `blob` endpoint and decrypts client-side.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use zim_core::linked_data::Link;
use zim_core::vault::VaultId;
use zim_peer::VaultLog;

use crate::access::can_access_vault;
use crate::http::auth::RequireUser;
use crate::state::AppState;

/// Capped pagination size so a single request can't walk a million-height
/// chain. Browser pages should fetch in chunks of ~20.
const MAX_LIMIT: u64 = 100;
const DEFAULT_LIMIT: u64 = 20;

#[derive(Debug, Deserialize)]
pub struct LogQuery {
    /// Highest height to include (inclusive). Defaults to current head.
    pub from: Option<u64>,
    /// How many heights to walk back (capped at `MAX_LIMIT`).
    pub limit: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct LogResponse {
    pub entries: Vec<LogEntry>,
}

#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub height: u64,
    pub link: Link,
}

pub async fn handler(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
    Path(vault_id): Path<VaultId>,
    Query(q): Query<LogQuery>,
) -> Response {
    if !can_access_vault(&state, &user, vault_id).await {
        return (StatusCode::NOT_FOUND, "vault not found").into_response();
    }
    // Read straight from coord.log() — the hub doesn't have a
    // Share, so the Vault-opener path can't be used.
    let log = state.service.peer().coord().log();
    let top = match q.from {
        Some(h) => h,
        None => match log.height(vault_id).await {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!("vault {vault_id} height: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "log lookup failed").into_response();
            }
        },
    };
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);

    let mut entries = Vec::with_capacity(limit as usize);
    let stop = top.saturating_sub(limit);
    let mut h = top;
    while h >= stop {
        match log.heads(vault_id, h).await {
            Ok(links) => {
                // Canonical head: lexicographically greatest link at
                // this height. Matches what `VaultLog::head` does.
                if let Some(link) = links.into_iter().max() {
                    entries.push(LogEntry { height: h, link });
                }
            }
            Err(e) => {
                tracing::warn!("vault {vault_id} heads({h}): {e}");
                break;
            }
        }
        if h == 0 {
            break;
        }
        h -= 1;
    }

    (StatusCode::OK, Json(LogResponse { entries })).into_response()
}
