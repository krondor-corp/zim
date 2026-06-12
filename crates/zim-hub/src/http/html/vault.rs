//! `/v/{vault_id}/tree/*path` — browser-side decrypted tree view.
//!
//! Server-side this is just a shell: vault id + display name. All
//! decryption happens client-side via zim-wasm's `WasmVault`
//! (`static/tree.js` drives it): the browser unlocks its web key
//! (IndexedDB → sessionStorage → escrow+passphrase), recovers the
//! vault secret from its manifest share, then walks the dir DAG via
//! `/api/v0/v/{id}/manifest` + `/blob/{hash}` fetches. The hub
//! serves ciphertext only.

use askama::Template;
use axum::extract::{Path, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use zim_core::vault::VaultId;

use crate::access::read_manifest_meta;
use crate::errors::Result;
use crate::http::auth::RequireOnboardedUser;
use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        // Three patterns so `tree`, `tree/`, and `tree/<any>` all
        // route the same. axum 0.7 doesn't normalize trailing slashes
        // between routes — each one has to be registered. The deep
        // pattern picks up a second `rest` path segment, so its
        // handler takes a tuple.
        .route("/:vault_id/tree", get(tree_root))
        .route("/:vault_id/tree/", get(tree_root))
        .route("/:vault_id/tree/*rest", get(tree_deep))
        .with_state(state)
}

#[derive(Template)]
#[template(path = "pages/vault-tree.html")]
struct TreeTemplate {
    hub_version: &'static str,
    user_email: String,
    is_admin: bool,
    active_nav: &'static str,
    vault_id: VaultId,
    vault_name: String,
}

async fn tree_root(
    State(state): State<AppState>,
    RequireOnboardedUser(user): RequireOnboardedUser,
    Path(vault_id): Path<VaultId>,
) -> Result<Html<String>> {
    tree(state, user, vault_id).await
}

async fn tree_deep(
    State(state): State<AppState>,
    RequireOnboardedUser(user): RequireOnboardedUser,
    Path((vault_id, _rest)): Path<(VaultId, String)>,
) -> Result<Html<String>> {
    tree(state, user, vault_id).await
}

async fn tree(
    state: AppState,
    user: crate::database::models::User,
    vault_id: VaultId,
) -> Result<Html<String>> {
    // Best-effort display name off the manifest; the per-blob API
    // endpoints do their own access gating, so a miss here only
    // affects the heading.
    let vault_name = read_manifest_meta(&state, vault_id)
        .await
        .map(|m| m.name)
        .unwrap_or_else(|| vault_id.to_string());

    let tmpl = TreeTemplate {
        hub_version: env!("CARGO_PKG_VERSION"),
        user_email: user.email().to_string(),
        is_admin: user.is_admin(),
        active_nav: "workspace",
        vault_id,
        vault_name,
    };
    Ok(Html(tmpl.render()?))
}
