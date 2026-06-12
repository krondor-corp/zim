//! `/app/devices` — list + remove the user's devices, link to setup.
//!
//! There's no paste-pubkey form anymore: typing a hex string can't
//! prove possession.
//!
//! - Browser-as-device runs the WebCrypto setup flow at
//!   `/app/devices/setup-this-browser`, which POSTs to
//!   `/api/v0/devices/self` with a signed possession proof. Gated
//!   by the session cookie.
//! - Daemons run the device-code flow via `zim login` and enroll
//!   atomically on the approved poll (see
//!   `crate::http::api::v0::auth`). They never touch
//!   `/api/v0/devices/self`.

use askama::Template;
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use serde::Deserialize;
use zim_crypto::PublicKey;

use crate::database::models::{UserPeer, UserPeerListItem};
use crate::errors::Result;
use crate::http::auth::RequireUser;
use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(list_page))
        .route("/setup-this-browser", get(this_browser_setup))
        .route("/:peer_pubkey/delete", post(delete))
        .with_state(state)
}

#[derive(Template)]
#[template(path = "pages/devices.html")]
struct DevicesTemplate {
    hub_version: &'static str,
    user_email: String,
    is_admin: bool,
    active_nav: &'static str,
    devices: Vec<DeviceRow>,
    flash: Option<String>,
}

struct DeviceRow {
    pubkey_hex: String,
    label: String,
    kind: String,
    created_at: String,
}

impl DeviceRow {
    fn from_listing(p: UserPeerListItem) -> Self {
        Self {
            pubkey_hex: p.peer_pubkey_hex().to_string(),
            label: p.label().to_string(),
            kind: p.kind().to_string(),
            created_at: p.created_at().to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub err: Option<String>,
}

async fn list_page(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
    Query(q): Query<ListQuery>,
) -> Result<Html<String>> {
    let devices = UserPeer::list_for_user(user.id(), &state.db)
        .await
        .map_err(|e| crate::errors::Error::Internal(format!("list devices: {e}")))?
        .into_iter()
        .map(DeviceRow::from_listing)
        .collect();
    let tmpl = DevicesTemplate {
        hub_version: env!("CARGO_PKG_VERSION"),
        user_email: user.email().to_string(),
        is_admin: user.is_admin(),
        active_nav: "devices",
        devices,
        flash: q.err,
    };
    Ok(Html(tmpl.render()?))
}

#[derive(Template)]
#[template(path = "pages/device-setup.html")]
struct DeviceSetupTemplate {
    hub_version: &'static str,
    user_email: String,
    is_admin: bool,
    active_nav: &'static str,
    user_id: uuid::Uuid,
    hub_host: String,
}

async fn this_browser_setup(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
) -> Result<Html<String>> {
    let tmpl = DeviceSetupTemplate {
        hub_version: env!("CARGO_PKG_VERSION"),
        user_email: user.email().to_string(),
        is_admin: user.is_admin(),
        active_nav: "devices",
        user_id: user.id(),
        hub_host: state.host.clone(),
    };
    Ok(Html(tmpl.render()?))
}

#[derive(Debug, Deserialize)]
pub struct DeletePath {
    pub peer_pubkey: String,
}

async fn delete(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
    axum::extract::Path(pubkey_hex): axum::extract::Path<String>,
) -> Response {
    let pk = match PublicKey::from_hex(&pubkey_hex) {
        Ok(p) => p,
        Err(_) => return redirect_with_error("invalid pubkey in path"),
    };
    match UserPeer::delete_for_user(user.id(), &pk, &state.db).await {
        Ok(true) => {
            tracing::info!(user = user.email(), pubkey = %pk.to_hex(), "device unregistered");
        }
        Ok(false) => {
            tracing::warn!(user = user.email(), pubkey = %pk.to_hex(), "device delete found nothing");
        }
        Err(e) => {
            tracing::warn!("device delete failed: {e}");
        }
    }
    Redirect::to("/app/devices").into_response()
}

fn redirect_with_error(msg: &str) -> Response {
    let encoded = urlencoding::encode(msg);
    Redirect::to(&format!("/app/devices?err={encoded}")).into_response()
}

// keep Form import alive for future possession-checked add forms
#[allow(dead_code)]
fn _force_form_import<T>(_: Form<T>) {}
