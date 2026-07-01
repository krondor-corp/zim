//! `/api/v0/admin/*` — user management as a JSON API for the SPA.
//!
//! Replaces the old server-rendered `/_admin` form-POST-and-redirect
//! handlers. Every route is gated by [`RequireAdmin`]; self-modifications
//! (unauthorize/demote yourself) are refused so an admin can't lock
//! themselves out mid-session.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use uuid::Uuid;

use crate::database::models::{User, UserPatch};
use crate::http::auth::RequireAdmin;
use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/users", get(list))
        .route("/users/:id/authorize", post(authorize))
        .route("/users/:id/unauthorize", post(unauthorize))
        .route("/users/:id/promote", post(promote))
        .route("/users/:id/demote", post(demote))
        .with_state(state)
}

#[derive(Serialize)]
struct AdminUser {
    id: String,
    email: String,
    name: String,
    role: String,
    is_admin: bool,
    is_authorized: bool,
}

#[derive(Serialize)]
struct UsersResponse {
    /// So the SPA can disable self-modifying buttons.
    current_admin_id: String,
    users: Vec<AdminUser>,
}

async fn list(RequireAdmin(admin): RequireAdmin, State(state): State<AppState>) -> Response {
    match User::list(&state.db).await {
        Ok(rows) => Json(UsersResponse {
            current_admin_id: admin.id().to_string(),
            users: rows
                .iter()
                .map(|u| AdminUser {
                    id: u.id().to_string(),
                    email: u.email().to_string(),
                    name: u.name().to_string(),
                    role: u.role().label().to_string(),
                    is_admin: u.is_admin(),
                    is_authorized: u.is_authorized(),
                })
                .collect(),
        })
        .into_response(),
        Err(e) => {
            tracing::error!("admin list users: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "list failed").into_response()
        }
    }
}

/// Apply a patch to `id`, returning 204 / 404 / 500.
async fn apply(state: &AppState, id: Uuid, patch: UserPatch) -> Response {
    let found = match User::find_by_id(id, &state.db).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(target_id = %id, "admin find: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "lookup failed").into_response();
        }
    };
    let Some(user) = found else {
        return (StatusCode::NOT_FOUND, "no such user").into_response();
    };
    match user.patch(patch, &state.db).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!(target_id = %id, "admin patch: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "update failed").into_response()
        }
    }
}

async fn authorize(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Response {
    tracing::info!(admin = admin.email(), target_id = %id, "user authorized");
    apply(
        &state,
        id,
        UserPatch {
            is_authorized: Some(true),
            ..Default::default()
        },
    )
    .await
}

async fn unauthorize(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Response {
    if admin.id() == id {
        return (StatusCode::BAD_REQUEST, "cannot unauthorize yourself").into_response();
    }
    tracing::info!(admin = admin.email(), target_id = %id, "user unauthorized");
    apply(
        &state,
        id,
        UserPatch {
            is_authorized: Some(false),
            ..Default::default()
        },
    )
    .await
}

async fn promote(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Response {
    tracing::info!(admin = admin.email(), target_id = %id, "user promoted to admin");
    // Promoting also authorizes, so the new admin has access immediately.
    apply(
        &state,
        id,
        UserPatch {
            is_admin: Some(true),
            is_authorized: Some(true),
            ..Default::default()
        },
    )
    .await
}

async fn demote(
    RequireAdmin(admin): RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Response {
    if admin.id() == id {
        return (StatusCode::BAD_REQUEST, "cannot demote yourself").into_response();
    }
    tracing::info!(admin = admin.email(), target_id = %id, "user demoted from admin");
    apply(
        &state,
        id,
        UserPatch {
            is_admin: Some(false),
            ..Default::default()
        },
    )
    .await
}
