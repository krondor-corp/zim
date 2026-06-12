//! `GET /` — hub landing page for an authorized user.
//!
//! Gated by [`RequireUser`]. Vault listing is filtered to vaults the
//! user owns (via the `user_peers` JOIN); admins see everything.

use askama::Template;
use axum::extract::State;
use axum::response::Html;
use zim_core::vault::VaultId;

use crate::access::{can_access_vault_via_db, read_manifest_meta};
use crate::errors::Result;
use crate::http::auth::RequireOnboardedUser;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "pages/index.html")]
struct IndexTemplate {
    hub_version: &'static str,
    user_email: String,
    is_admin: bool,
    active_nav: &'static str,
    vaults: Vec<VaultRow>,
}

struct VaultRow {
    id: VaultId,
    name: String,
    error: Option<String>,
}

pub async fn handler(
    State(state): State<AppState>,
    RequireOnboardedUser(user): RequireOnboardedUser,
) -> Result<Html<String>> {
    let listings = state
        .service
        .peer()
        .list_vaults()
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("list_vaults failed: {e}");
            Vec::new()
        });

    let mut vaults = Vec::new();
    for v in listings {
        // Always go through the raw-manifest path: it gives us both
        // the shareholder list (for the ownership JOIN) and the
        // human-readable name without needing a `Share`. The
        // VaultListing's own `name` field is `None` for relay-only
        // mirrors (Peer::vault fails with ShareNotFound), so this
        // is the only way to render a useful name on the hub.
        let Some(meta) = read_manifest_meta(&state, v.id).await else {
            continue;
        };
        let visible =
            user.is_admin() || can_access_vault_via_db(&state.db, &user, &meta.shareholders).await;
        if !visible {
            continue;
        }
        // Drop the per-row `error` from `list_vaults`. The hub is
        // never a shareholder, so every well-mirrored vault returns
        // `ShareNotFound` here — surfacing that to the user as a
        // card-level "error" badge is just noise. If
        // `read_manifest_meta` succeeded above, the vault is
        // healthy as far as the hub's responsibilities go.
        vaults.push(VaultRow {
            id: v.id,
            name: meta.name,
            error: None,
        });
    }

    let tmpl = IndexTemplate {
        hub_version: env!("CARGO_PKG_VERSION"),
        user_email: user.email().to_string(),
        is_admin: user.is_admin(),
        active_nav: "workspace",
        vaults,
    };
    Ok(Html(tmpl.render()?))
}
