use std::path::Path;
use std::sync::Arc;

use parking_lot::{Mutex, MutexGuard};
use rusqlite::Connection;

use super::error::Result;
use super::migrations::migrations;

#[derive(Debug, Clone)]
pub(crate) struct Database {
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

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock()
    }
}
