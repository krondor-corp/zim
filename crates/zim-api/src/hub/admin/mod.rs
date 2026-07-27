//! `/api/v0/admin/*` — user management (RequireAdmin). The hub server
//! serializes these; the web SPA's admin page drives them.

pub mod action;
pub mod users;

pub use action::AdminActionRequest;
pub use users::AdminUsersRequest;
pub use users::{AdminUser, AdminUsers};
