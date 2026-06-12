use serde::{Deserialize, Serialize};
use struct_patch::Patch;
use uuid::Uuid;

use crate::database::types::DbUuid;
use crate::database::Database;

/// One Google-authenticated user of the hub.
///
/// Roles are a pair of independent boolean flags:
///
/// - `is_admin` — can read `/_admin`, promote/demote others.
/// - `is_authorized` — can access the workspace UI / API.
///
/// `is_admin` implies workspace access regardless of
/// `is_authorized`; admins are always usable. The pending state is
/// "row exists, neither flag set." Bootstrapping happens in the
/// callback handler: emails in `config.auth.admin_emails` are
/// inserted with both flags `true`.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, Patch)]
#[patch(attribute(derive(Debug, Default, Deserialize)))]
pub struct User {
    #[patch(skip)]
    id: DbUuid,
    email: String,
    name: String,
    is_admin: bool,
    is_authorized: bool,
    #[patch(skip)]
    created_at: String,
    #[patch(skip)]
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct UserListItem {
    id: DbUuid,
    email: String,
    name: String,
    is_admin: bool,
    is_authorized: bool,
    created_at: String,
    updated_at: String,
}

impl UserListItem {
    pub fn id(&self) -> Uuid {
        self.id.into()
    }
    pub fn email(&self) -> &str {
        &self.email
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn is_admin(&self) -> bool {
        self.is_admin
    }
    pub fn is_authorized(&self) -> bool {
        self.is_authorized
    }
    pub fn role(&self) -> Role {
        Role::resolve(self.is_admin, self.is_authorized)
    }
    pub fn created_at(&self) -> &str {
        &self.created_at
    }
    pub fn updated_at(&self) -> &str {
        &self.updated_at
    }
}

impl User {
    pub fn id(&self) -> Uuid {
        self.id.into()
    }
    pub fn email(&self) -> &str {
        &self.email
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn is_admin(&self) -> bool {
        self.is_admin
    }
    pub fn is_authorized(&self) -> bool {
        self.is_authorized
    }
    pub fn role(&self) -> Role {
        Role::resolve(self.is_admin, self.is_authorized)
    }
    /// Admins always count as usable; otherwise gated by
    /// `is_authorized`.
    pub fn can_use_app(&self) -> bool {
        self.is_admin || self.is_authorized
    }
    pub fn created_at(&self) -> &str {
        &self.created_at
    }
    pub fn updated_at(&self) -> &str {
        &self.updated_at
    }

    pub async fn create(
        email: &str,
        name: &str,
        is_admin: bool,
        is_authorized: bool,
        db: &Database,
    ) -> Result<Self, sqlx::Error> {
        let id = DbUuid::new();
        sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (id, email, name, is_admin, is_authorized)
            VALUES (?, ?, ?, ?, ?)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(email)
        .bind(name)
        .bind(is_admin)
        .bind(is_authorized)
        .fetch_one(&**db)
        .await
    }

    pub async fn patch(mut self, patch: UserPatch, db: &Database) -> Result<Self, sqlx::Error> {
        self.apply(patch);
        sqlx::query_as::<_, User>(
            r#"
            UPDATE users
            SET email = ?, name = ?, is_admin = ?, is_authorized = ?,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
            RETURNING *
            "#,
        )
        .bind(&self.email)
        .bind(&self.name)
        .bind(self.is_admin)
        .bind(self.is_authorized)
        .bind(self.id)
        .fetch_one(&**db)
        .await
    }

    pub async fn find_by_email(email: &str, db: &Database) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = ?")
            .bind(email)
            .fetch_optional(&**db)
            .await
    }

    pub async fn find_by_id(id: Uuid, db: &Database) -> Result<Option<Self>, sqlx::Error> {
        let db_id = DbUuid::from(id);
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
            .bind(db_id)
            .fetch_optional(&**db)
            .await
    }

    pub async fn list(db: &Database) -> Result<Vec<UserListItem>, sqlx::Error> {
        sqlx::query_as::<_, UserListItem>("SELECT * FROM users ORDER BY created_at DESC")
            .fetch_all(&**db)
            .await
    }

    pub async fn count(db: &Database) -> Result<i64, sqlx::Error> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&**db)
            .await?;
        Ok(row.0)
    }
}

/// Display-level role enum derived from the two boolean flags.
/// Tables / templates branch on this rather than re-deriving the
/// same logic per call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Pending,
    Authorized,
    Admin,
}

impl Role {
    fn resolve(is_admin: bool, is_authorized: bool) -> Self {
        if is_admin {
            Role::Admin
        } else if is_authorized {
            Role::Authorized
        } else {
            Role::Pending
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Role::Pending => "pending",
            Role::Authorized => "authorized",
            Role::Admin => "admin",
        }
    }
}
