//! Hub-side persistent state.
//!
//! Shape mirrors the `generic` reference project: a [`Database`]
//! newtype around an `sqlx` `SqlitePool` that `Deref`s to it, plus
//! a `models/` submodule with one file per table. Each model is a
//! private-field struct with getter methods + a few async DB
//! operations (`find_*`, `create`, `patch`, `list`). Typed UUID
//! columns go through [`types::DbUuid`].
//!
//! Connection + migration helpers live in [`sqlite`].

pub mod models;
mod sqlite;
pub mod types;

use std::ops::Deref;

use sqlx::SqlitePool;

#[derive(Clone, Debug)]
pub struct Database(SqlitePool);

impl Database {
    pub async fn connect(database_url: &url::Url) -> Result<Self, DatabaseSetupError> {
        let pool = sqlite::connect(database_url).await?;
        sqlite::migrate(&pool).await?;
        Ok(Self(pool))
    }
}

impl Deref for Database {
    type Target = SqlitePool;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DatabaseSetupError {
    #[error("migration failed: {0}")]
    MigrationFailed(sqlx::migrate::MigrateError),
    #[error("database unavailable: {0}")]
    Unavailable(sqlx::Error),
}
