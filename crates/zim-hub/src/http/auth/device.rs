//! `/auth/device` — approve a daemon's `zim hub login` device code.
//! Server-rendered, like [`super::login`]: approval is auth, not key
//! custody, so it never touches the wasm app (or its web-key gate).
//!
//! - `GET /auth/device?code=…` (RequireUser) shows the grant's label +
//!   pubkey and an Approve button. Anonymous visitors sign in first and
//!   come back here via `next`.
//! - `POST /auth/device/approve` (form) stamps the grant; the daemon's
//!   next signed poll (`/api/v0/auth/device-code/poll`) enrolls it. A
//!   POST to self carrying a `SameSite=Lax` session cookie, so another
//!   site can't approve on the user's behalf.
//! - `GET /device?code=…` — where codes minted before this page moved
//!   pointed. Redirects here so in-flight logins survive.

use askama::Template;
use axum::extract::{Query, RawQuery, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use serde::Deserialize;

use crate::database::models::DeviceCodeGrant;
use crate::http::auth::{return_to, OptionalUser, RequireUser, PENDING_PATH};
use crate::state::AppState;
use crate::Database;

pub const DEVICE_PATH: &str = "/auth/device";

#[derive(Deserialize)]
pub struct CodeQuery {
    #[serde(default)]
    code: String,
}

/// `GET /auth/device?code=…` — the grant the user is about to approve.
pub async fn page(
    State(state): State<AppState>,
    RequireUser(_user): RequireUser,
    Query(query): Query<CodeQuery>,
) -> Response {
    match lookup(query.code.trim(), &state.db).await {
        Ok(view) => view.into_response(),
        Err(e) => {
            tracing::error!("device-code lookup: {e}");
            View::Failed.into_response()
        }
    }
}

/// `POST /auth/device/approve` — approve the posted code as the signed-in
/// user. A session that lapsed while the page sat open signs in again and
/// lands back on the approval page, not on this POST-only route.
pub async fn approve(
    State(state): State<AppState>,
    OptionalUser(user): OptionalUser,
    Form(form): Form<CodeQuery>,
) -> Response {
    let code = form.code.trim();
    let user = match user {
        None => return Redirect::to(&return_to::login_url(&page_url(code))).into_response(),
        Some(u) if !u.can_use_app() => return Redirect::to(PENDING_PATH).into_response(),
        Some(u) => u,
    };
    match approve_as(code, user.id(), &state.db).await {
        Ok(view) => {
            if view == View::Approved {
                tracing::info!(user = user.email(), code, "device-code approved");
            }
            view.into_response()
        }
        Err(e) => {
            tracing::error!("device-code approve: {e}");
            View::Failed.into_response()
        }
    }
}

/// `GET /device?…` — the pre-`/auth/device` verification URL.
pub async fn legacy_redirect(RawQuery(query): RawQuery) -> Redirect {
    match query {
        Some(q) => Redirect::to(&format!("{DEVICE_PATH}?{q}")),
        None => Redirect::to(DEVICE_PATH),
    }
}

/// The approval page for `code`.
fn page_url(code: &str) -> String {
    format!("{DEVICE_PATH}?code={}", urlencoding::encode(code))
}

/// What the device page shows.
#[derive(Debug, PartialEq)]
enum View {
    /// Opened without `?code=`.
    NoCode,
    NotFound,
    Expired,
    /// Waiting on this user's click.
    Pending {
        code: String,
        label: String,
        pubkey: String,
    },
    /// Approved (by this user) — the daemon will finish on its own.
    Approved,
    /// Approved by someone else, or consumed while the page sat open.
    Taken,
    Failed,
}

async fn lookup(code: &str, db: &Database) -> Result<View, sqlx::Error> {
    if code.is_empty() {
        return Ok(View::NoCode);
    }
    Ok(match DeviceCodeGrant::find(code, db).await? {
        None => View::NotFound,
        Some(g) if g.is_expired() => View::Expired,
        Some(g) if g.is_approved() => View::Taken,
        Some(g) => View::Pending {
            code: g.code().to_string(),
            label: g.label().to_string(),
            pubkey: g.pubkey_hex().to_string(),
        },
    })
}

async fn approve_as(code: &str, user_id: uuid::Uuid, db: &Database) -> Result<View, sqlx::Error> {
    if code.is_empty() {
        return Ok(View::NoCode);
    }
    let grant = match DeviceCodeGrant::find(code, db).await? {
        None => return Ok(View::NotFound),
        Some(g) if g.is_expired() => return Ok(View::Expired),
        Some(g) => g,
    };
    // A resubmitted form (refresh, double click) is still a success.
    if grant.is_approved() {
        return Ok(if grant.user_id() == Some(user_id) {
            View::Approved
        } else {
            View::Taken
        });
    }
    Ok(if DeviceCodeGrant::approve(code, user_id, db).await? {
        View::Approved
    } else {
        View::Taken
    })
}

impl View {
    fn status(&self) -> StatusCode {
        match self {
            View::NoCode => StatusCode::BAD_REQUEST,
            View::NotFound => StatusCode::NOT_FOUND,
            View::Expired | View::Taken => StatusCode::GONE,
            View::Failed => StatusCode::INTERNAL_SERVER_ERROR,
            View::Pending { .. } | View::Approved => StatusCode::OK,
        }
    }

    fn heading(&self) -> &'static str {
        match self {
            View::NoCode | View::Pending { .. } => "Approve a device",
            View::NotFound => "Code not found",
            View::Expired => "Code expired",
            View::Taken => "Code already used",
            View::Failed => "Something went wrong",
            View::Approved => "Device approved",
        }
    }
}

/// `templates/auth/device.html`: one page, a body per [`View`].
#[derive(Template)]
#[template(path = "auth/device.html")]
struct DevicePage {
    heading: &'static str,
    view: View,
}

impl IntoResponse for View {
    fn into_response(self) -> Response {
        let status = self.status();
        let page = DevicePage {
            heading: self.heading(),
            view: self,
        };
        (status, page).into_response()
    }
}

/// The page as HTML, for tests.
#[cfg(test)]
fn render(view: View) -> String {
    DevicePage {
        heading: view.heading(),
        view,
    }
    .render()
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::user;
    use zim_crypto::PrivateKey;

    /// A fresh `zim hub login` from Alice's laptop.
    async fn laptop_login(db: &Database, label: &str) -> DeviceCodeGrant {
        let pubkey = PrivateKey::generate().public().to_hex();
        DeviceCodeGrant::create(&pubkey, label, db).await.unwrap()
    }

    #[tokio::test]
    async fn alice_approves_her_laptop_from_the_printed_link() {
        let db = Database::in_memory().await;
        let alice = user(&db, "alice").await;
        let grant = laptop_login(&db, "alices-laptop").await;

        // The page shows what the terminal shows.
        let html = render(lookup(grant.code(), &db).await.unwrap());
        assert!(html.contains("alices-laptop"));
        assert!(html.contains(grant.code()));
        assert!(html.contains(grant.pubkey_hex()));
        assert!(html.contains(r#"action="/auth/device/approve""#));

        // Approve → the daemon's next poll can enroll.
        let view = approve_as(grant.code(), alice.id(), &db).await.unwrap();
        assert_eq!(view, View::Approved);
        let stamped = DeviceCodeGrant::find(grant.code(), &db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stamped.user_id(), Some(alice.id()));
    }

    #[tokio::test]
    async fn a_double_submit_is_still_a_success_for_alice_but_not_for_bob() {
        let db = Database::in_memory().await;
        let alice = user(&db, "alice").await;
        let bob = user(&db, "bob").await;
        let grant = laptop_login(&db, "alices-laptop").await;
        approve_as(grant.code(), alice.id(), &db).await.unwrap();

        // Alice refreshes the done page.
        assert_eq!(
            approve_as(grant.code(), alice.id(), &db).await.unwrap(),
            View::Approved
        );
        // Bob can't steal (or re-approve) Alice's grant.
        assert_eq!(
            approve_as(grant.code(), bob.id(), &db).await.unwrap(),
            View::Taken
        );
        let stamped = DeviceCodeGrant::find(grant.code(), &db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stamped.user_id(), Some(alice.id()));
    }

    #[tokio::test]
    async fn a_mistyped_or_missing_code_says_so() {
        let db = Database::in_memory().await;
        let alice = user(&db, "alice").await;
        assert_eq!(lookup("", &db).await.unwrap(), View::NoCode);
        assert_eq!(lookup("NOPE-NOPE", &db).await.unwrap(), View::NotFound);
        assert_eq!(
            approve_as("NOPE-NOPE", alice.id(), &db).await.unwrap(),
            View::NotFound
        );
    }

    #[tokio::test]
    async fn mallorys_device_label_cannot_inject_markup() {
        let db = Database::in_memory().await;
        let grant = laptop_login(&db, r#""><script>alert(1)</script>"#).await;
        let html = render(lookup(grant.code(), &db).await.unwrap());
        assert!(!html.contains("<script>"));
        assert!(html.contains("&quot;&gt;&lt;script&gt;"));
    }

    #[test]
    fn an_expired_session_signs_in_and_returns_to_the_approval_page() {
        assert_eq!(
            return_to::login_url(&page_url("ABCD-EFGH")),
            "/auth/login?next=%2Fauth%2Fdevice%3Fcode%3DABCD-EFGH"
        );
    }

    #[tokio::test]
    async fn a_link_minted_before_the_move_still_lands_on_the_approval_page() {
        let r = legacy_redirect(RawQuery(Some("code=ABCD-EFGH".into())))
            .await
            .into_response();
        assert_eq!(
            r.headers()[axum::http::header::LOCATION],
            "/auth/device?code=ABCD-EFGH"
        );
    }
}
