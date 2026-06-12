//! Role-change POST endpoints. Each handler: gate via
//! [`RequireAdmin`], take the target user's id off the path, refuse
//! self-modifications (so an admin doesn't lock themselves out
//! mid-session), apply the change via [`User::patch`], redirect to
//! `/_admin`.

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use uuid::Uuid;

use crate::database::models::{User, UserPatch};
use crate::http::auth::RequireAdmin;
use crate::state::AppState;

const DASHBOARD: &str = "/_admin";

async fn apply_patch(
    state: &AppState,
    target_id: Uuid,
    patch: UserPatch,
) -> Result<(), sqlx::Error> {
    let Some(user) = User::find_by_id(target_id, &state.db).await? else {
        return Ok(());
    };
    user.patch(patch, &state.db).await.map(|_| ())
}

pub async fn authorize(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Response {
    let patch = UserPatch {
        is_authorized: Some(true),
        ..Default::default()
    };
    if let Err(e) = apply_patch(&state, id, patch).await {
        tracing::warn!(target_id = %id, "authorize failed: {e}");
    } else {
        tracing::info!(admin = admin.email(), target_id = %id, "user authorized");
    }
    Redirect::to(DASHBOARD).into_response()
}

pub async fn unauthorize(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Response {
    if admin.id() == id {
        return Redirect::to(DASHBOARD).into_response();
    }
    let patch = UserPatch {
        is_authorized: Some(false),
        ..Default::default()
    };
    if let Err(e) = apply_patch(&state, id, patch).await {
        tracing::warn!(target_id = %id, "unauthorize failed: {e}");
    } else {
        tracing::info!(admin = admin.email(), target_id = %id, "user unauthorized");
    }
    Redirect::to(DASHBOARD).into_response()
}

pub async fn promote(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Response {
    // Promoting also flips `is_authorized` so the new admin has
    // workspace access immediately (otherwise an admin could be
    // simultaneously "admin" and "pending app access").
    let patch = UserPatch {
        is_admin: Some(true),
        is_authorized: Some(true),
        ..Default::default()
    };
    if let Err(e) = apply_patch(&state, id, patch).await {
        tracing::warn!(target_id = %id, "promote failed: {e}");
    } else {
        tracing::info!(admin = admin.email(), target_id = %id, "user promoted to admin");
    }
    Redirect::to(DASHBOARD).into_response()
}

pub async fn demote(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Response {
    if admin.id() == id {
        return Redirect::to(DASHBOARD).into_response();
    }
    let patch = UserPatch {
        is_admin: Some(false),
        ..Default::default()
    };
    if let Err(e) = apply_patch(&state, id, patch).await {
        tracing::warn!(target_id = %id, "demote failed: {e}");
    } else {
        tracing::info!(admin = admin.email(), target_id = %id, "user demoted from admin");
    }
    Redirect::to(DASHBOARD).into_response()
}
