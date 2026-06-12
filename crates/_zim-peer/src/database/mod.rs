pub mod bucket_log_provider;
mod migrations;
pub mod models;
pub mod types;

use std::path::Path;
use std::sync::Arc;

use parking_lot::{Mutex, MutexGuard};
use rusqlite::Connection;

use migrations::migrations;

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("{0}")]
    Client(#[from] rusqlite::Error),
    #[error("{0}")]
    Deserialize(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, DatabaseError>;

#[derive(Clone, Debug)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn new(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(anyhow::Error::from)?;
        }
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        migrations()
            .to_latest(&mut conn)
            .map_err(anyhow::Error::from)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        migrations()
            .to_latest(&mut conn)
            .map_err(anyhow::Error::from)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn connect(database_url: &url::Url) -> std::result::Result<Self, DatabaseSetupError> {
        if database_url.scheme() != "sqlite" {
            return Err(DatabaseSetupError::UnknownDbType(
                database_url.scheme().to_string(),
            ));
        }

        let path_str = database_url.path();
        if path_str.contains(":memory:") || path_str.contains("mode=memory") {
            Self::in_memory().map_err(DatabaseSetupError::from)
        } else {
            Self::new(Path::new(path_str)).map_err(DatabaseSetupError::from)
        }
    }

    /// Create a unique in-memory database (for tests).
    pub fn memory() -> std::result::Result<Self, DatabaseSetupError> {
        Self::in_memory().map_err(DatabaseSetupError::from)
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock()
    }

    /// Run a sync database closure on the blocking threadpool.
    pub async fn blocking<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Database) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let db = self.clone();
        tokio::task::spawn_blocking(move || f(&db))
            .await
            .map_err(|e| DatabaseError::Deserialize(e.into()))?
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DatabaseSetupError {
    #[error("database error: {0}")]
    Database(#[from] DatabaseError),

    #[error("requested database type was not recognized: {0}")]
    UnknownDbType(String),
}
