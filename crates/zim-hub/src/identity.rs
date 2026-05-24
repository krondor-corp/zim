//! zim-hub-side identity store.
//!
//! Separate SQLite database from the embedded peer's `zim-hub.db` — keeps the
//! identity-vault concern (viewer keys, sessions) physically separated from the
//! peer protocol's bucket log. Migrations live in `crates/zim-hub/migrations/`.
//!
//! M1 (T-001a): pool + migrations only. Enrolment / unlock / rekey queries
//! land in subsequent milestones.

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

#[derive(Debug, thiserror::Error)]
pub enum IdentityStoreError {
    #[error("connect: {0}")]
    Connect(sqlx::Error),
    #[error("migrate: {0}")]
    Migrate(sqlx::migrate::MigrateError),
}

#[derive(Clone)]
pub struct IdentityStore {
    pool: SqlitePool,
}

impl IdentityStore {
    /// Open (or create) the identity database at `path`, then run migrations.
    pub async fn open(path: &Path) -> Result<Self, IdentityStoreError> {
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await
            .map_err(IdentityStoreError::Connect)?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(IdentityStoreError::Migrate)?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
