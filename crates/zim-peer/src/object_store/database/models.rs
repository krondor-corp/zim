use anyhow::anyhow;
use rusqlite::{params, OptionalExtension};

use super::client::Database;
use super::error::Result;

/// State of a blob in the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum BlobState {
    #[default]
    Complete,
    Partial,
    Deleting,
}

impl BlobState {
    pub fn as_str(&self) -> &'static str {
        match self {
            BlobState::Complete => "complete",
            BlobState::Partial => "partial",
            BlobState::Deleting => "deleting",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "complete" => Ok(BlobState::Complete),
            "partial" => Ok(BlobState::Partial),
            "deleting" => Ok(BlobState::Deleting),
            other => Err(anyhow!("unknown blob state: {other}").into()),
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct BlobMetadata {
    pub hash: String,
    pub size: i64,
    pub has_outboard: bool,
    pub state: BlobState,
    pub created_at: i64,
    pub updated_at: i64,
}

impl BlobMetadata {
    /// Insert or update a blob record as complete.
    pub fn insert(db: &Database, hash: &str, size: i64, has_outboard: bool) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO blobs (hash, size, has_outboard, state, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(hash) DO UPDATE SET
                 size = excluded.size,
                 has_outboard = excluded.has_outboard,
                 state = excluded.state,
                 updated_at = excluded.updated_at",
            params![
                hash,
                size,
                has_outboard,
                BlobState::Complete.as_str(),
                now,
                now
            ],
        )?;
        Ok(())
    }

    /// Insert a blob record in partial state (import in progress).
    pub fn insert_partial(db: &Database, hash: &str, size: i64, has_outboard: bool) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO blobs (hash, size, has_outboard, state, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(hash) DO UPDATE SET
                 size = excluded.size,
                 has_outboard = excluded.has_outboard,
                 state = CASE
                     WHEN blobs.state = 'complete' THEN 'complete'
                     ELSE excluded.state
                 END,
                 updated_at = excluded.updated_at",
            params![
                hash,
                size,
                has_outboard,
                BlobState::Partial.as_str(),
                now,
                now
            ],
        )?;
        Ok(())
    }

    /// Get the state of a blob.
    pub fn get_state(db: &Database, hash: &str) -> Result<Option<BlobState>> {
        let conn = db.conn();
        let state_str: Option<String> = conn
            .query_row(
                "SELECT state FROM blobs WHERE hash = ?1",
                params![hash],
                |row| row.get(0),
            )
            .optional()?;
        match state_str {
            Some(s) => Ok(Some(BlobState::parse(&s)?)),
            None => Ok(None),
        }
    }

    /// Get blob metadata by hash.
    pub fn get(db: &Database, hash: &str) -> Result<Option<BlobMetadata>> {
        let conn = db.conn();
        let row: Option<(String, i64, bool, String, i64, i64)> = conn
            .query_row(
                "SELECT hash, size, has_outboard, state, created_at, updated_at
                 FROM blobs WHERE hash = ?1",
                params![hash],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .optional()?;
        match row {
            Some((hash, size, has_outboard, state_str, created_at, updated_at)) => {
                Ok(Some(BlobMetadata {
                    hash,
                    size,
                    has_outboard,
                    state: BlobState::parse(&state_str)?,
                    created_at,
                    updated_at,
                }))
            }
            None => Ok(None),
        }
    }

    /// Check if a complete blob exists.
    pub fn exists(db: &Database, hash: &str) -> Result<bool> {
        let conn = db.conn();
        let result = conn
            .query_row(
                "SELECT 1 FROM blobs WHERE hash = ?1 AND state = ?2",
                params![hash, BlobState::Complete.as_str()],
                |_| Ok(()),
            )
            .optional()?;
        Ok(result.is_some())
    }

    /// Delete a blob record.
    pub fn delete(db: &Database, hash: &str) -> Result<bool> {
        let conn = db.conn();
        let count = conn.execute("DELETE FROM blobs WHERE hash = ?1", params![hash])?;
        Ok(count > 0)
    }

    /// List all complete blob hashes.
    pub fn list_hashes(db: &Database) -> Result<Vec<String>> {
        let conn = db.conn();
        let mut stmt =
            conn.prepare("SELECT hash FROM blobs WHERE state = ?1 ORDER BY created_at DESC")?;
        let rows = stmt
            .query_map(params![BlobState::Complete.as_str()], |row| row.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
impl BlobMetadata {
    pub fn mark_complete(db: &Database, hash: &str) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let conn = db.conn();
        let count = conn.execute(
            "UPDATE blobs SET state = ?1, updated_at = ?2
             WHERE hash = ?3 AND state = ?4",
            params![
                BlobState::Complete.as_str(),
                now,
                hash,
                BlobState::Partial.as_str()
            ],
        )?;
        Ok(count > 0)
    }

    pub fn count(db: &Database) -> Result<i64> {
        let conn = db.conn();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM blobs WHERE state = ?1",
            params![BlobState::Complete.as_str()],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn total_size(db: &Database) -> Result<i64> {
        let conn = db.conn();
        let total: i64 = conn.query_row(
            "SELECT COALESCE(SUM(size), 0) FROM blobs WHERE state = ?1",
            params![BlobState::Complete.as_str()],
            |row| row.get(0),
        )?;
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_database() {
        let db = Database::in_memory().unwrap();

        BlobMetadata::insert(&db, "abc123", 1024, false).unwrap();

        assert!(BlobMetadata::exists(&db, "abc123").unwrap());

        let meta = BlobMetadata::get(&db, "abc123").unwrap().unwrap();
        assert_eq!(meta.hash, "abc123");
        assert_eq!(meta.size, 1024);
        assert!(!meta.has_outboard);

        let blobs = BlobMetadata::list_hashes(&db).unwrap();
        assert_eq!(blobs.len(), 1);
        assert_eq!(blobs[0], "abc123");

        assert_eq!(BlobMetadata::count(&db).unwrap(), 1);
        assert_eq!(BlobMetadata::total_size(&db).unwrap(), 1024);

        assert!(BlobMetadata::delete(&db, "abc123").unwrap());
        assert!(!BlobMetadata::exists(&db, "abc123").unwrap());
    }

    #[test]
    fn test_upsert_blob() {
        let db = Database::in_memory().unwrap();

        BlobMetadata::insert(&db, "abc123", 1024, false).unwrap();
        BlobMetadata::insert(&db, "abc123", 2048, true).unwrap();

        let meta = BlobMetadata::get(&db, "abc123").unwrap().unwrap();
        assert_eq!(meta.size, 2048);
        assert!(meta.has_outboard);

        assert_eq!(BlobMetadata::count(&db).unwrap(), 1);
    }

    #[test]
    fn test_partial_blob_lifecycle() {
        let db = Database::in_memory().unwrap();

        BlobMetadata::insert_partial(&db, "hash1", 4096, true).unwrap();

        let state = BlobMetadata::get_state(&db, "hash1").unwrap();
        assert_eq!(state, Some(BlobState::Partial));
        assert!(!BlobMetadata::exists(&db, "hash1").unwrap());
        assert_eq!(BlobMetadata::list_hashes(&db).unwrap().len(), 0);

        assert!(BlobMetadata::mark_complete(&db, "hash1").unwrap());

        let state = BlobMetadata::get_state(&db, "hash1").unwrap();
        assert_eq!(state, Some(BlobState::Complete));
        assert!(BlobMetadata::exists(&db, "hash1").unwrap());
        assert_eq!(BlobMetadata::list_hashes(&db).unwrap().len(), 1);
    }

    #[test]
    fn test_partial_blob_no_overwrite_complete() {
        let db = Database::in_memory().unwrap();

        BlobMetadata::insert(&db, "hash1", 4096, true).unwrap();
        assert_eq!(
            BlobMetadata::get_state(&db, "hash1").unwrap(),
            Some(BlobState::Complete)
        );

        BlobMetadata::insert_partial(&db, "hash1", 4096, true).unwrap();
        assert_eq!(
            BlobMetadata::get_state(&db, "hash1").unwrap(),
            Some(BlobState::Complete)
        );
    }

    #[test]
    fn test_get_blob_state_nonexistent() {
        let db = Database::in_memory().unwrap();
        assert_eq!(BlobMetadata::get_state(&db, "nonexistent").unwrap(), None);
    }

    #[test]
    fn test_parse_invalid_state() {
        let result = BlobState::parse("bogus");
        assert!(result.is_err());
    }
}
