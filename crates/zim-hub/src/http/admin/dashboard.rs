//! `GET /_admin` — user list with role badges + role-change buttons.

use askama::Template;
use axum::extract::State;
use axum::response::Html;
use uuid::Uuid;

use crate::database::models::{User, UserListItem};
use crate::errors::Result;
use crate::http::auth::RequireAdmin;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "pages/admin/dashboard.html")]
struct AdminDashboardTemplate {
    hub_version: &'static str,
    user_email: String,
    is_admin: bool,
    active_nav: &'static str,
    current_admin_id: Uuid,
    users: Vec<UserRow>,
}

/// Pre-rendered view of one row in the dashboard table.
struct UserRow {
    id: Uuid,
    email: String,
    name: String,
    role_label: &'static str,
    is_admin: bool,
    is_authorized: bool,
}

impl UserRow {
    fn from_listing(u: UserListItem) -> Self {
        let role_label = u.role().label();
        Self {
            id: u.id(),
            email: u.email().to_string(),
            name: u.name().to_string(),
            role_label,
            is_admin: u.is_admin(),
            is_authorized: u.is_authorized(),
        }
    }
}

pub async fn handler(
    State(state): State<AppState>,
    RequireAdmin(admin): RequireAdmin,
) -> Result<Html<String>> {
    let users = User::list(&state.db)
        .await
        .map_err(|e| crate::errors::Error::Internal(format!("list users: {e}")))?
        .into_iter()
        .map(UserRow::from_listing)
        .collect();
    let tmpl = AdminDashboardTemplate {
        hub_version: env!("CARGO_PKG_VERSION"),
        user_email: admin.email().to_string(),
        is_admin: true,
        active_nav: "admin",
        current_admin_id: admin.id(),
        users,
    };
    Ok(Html(tmpl.render()?))
}
