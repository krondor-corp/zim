//! Sync target model for daemon-managed filesystem backup sync (T-018).
//!
//! Pattern mirrors `FuseMount`: one row per registered backup target.

use std::str::FromStr;

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::database::Database;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncStatus {
    Active,
    Paused,
    Error,
}

impl SyncStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncStatus::Active => "active",
            SyncStatus::Paused => "paused",
            SyncStatus::Error => "error",
        }
    }
}

impl std::fmt::Display for SyncStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SyncStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(SyncStatus::Active),
            "paused" => Ok(SyncStatus::Paused),
            "error" => Ok(SyncStatus::Error),
            other => Err(format!("unknown SyncStatus: {other}")),
        }
    }
}

impl ToSql for SyncStatus {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

impl FromSql for SyncStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let s = value.as_str()?;
        SyncStatus::from_str(s).map_err(|e| FromSqlError::Other(e.into()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncTarget {
    pub id: Uuid,
    pub bucket_id: Uuid,
    pub target_path: String,
    pub last_head: Option<String>,
    pub last_sync: Option<i64>,
    pub status: SyncStatus,
    pub error_message: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncTarget> {
    Ok(SyncTarget {
        id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
        bucket_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap(),
        target_path: row.get(2)?,
        last_head: row.get(3)?,
        last_sync: row.get(4)?,
        status: row.get(5)?,
        error_message: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

impl SyncTarget {
    pub fn create(
        bucket_id: Uuid,
        target_path: &str,
        db: &Database,
    ) -> crate::database::Result<SyncTarget> {
        let id = Uuid::new_v4();
        let conn = db.conn();

        conn.execute(
            r#"
            INSERT INTO sync_targets (id, bucket_id, target_path)
            VALUES (?1, ?2, ?3)
            "#,
            rusqlite::params![id.to_string(), bucket_id.to_string(), target_path],
        )?;

        drop(conn);
        Self::get(id, db)?.ok_or_else(|| {
            crate::database::DatabaseError::Client(rusqlite::Error::QueryReturnedNoRows)
        })
    }

    pub fn get(id: Uuid, db: &Database) -> crate::database::Result<Option<SyncTarget>> {
        let conn = db.conn();
        let result = conn
            .query_row(
                "SELECT id, bucket_id, target_path, last_head, last_sync, status, error_message, created_at, updated_at FROM sync_targets WHERE id = ?1",
                rusqlite::params![id.to_string()],
                map_row,
            )
            .optional()?;
        Ok(result)
    }

    pub fn list(db: &Database) -> crate::database::Result<Vec<SyncTarget>> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, bucket_id, target_path, last_head, last_sync, status, error_message, created_at, updated_at FROM sync_targets ORDER BY created_at DESC",
        )?;
        let rows = stmt
            .query_map([], map_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn list_by_bucket(
        bucket_id: Uuid,
        db: &Database,
    ) -> crate::database::Result<Vec<SyncTarget>> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, bucket_id, target_path, last_head, last_sync, status, error_message, created_at, updated_at FROM sync_targets WHERE bucket_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt
            .query_map(rusqlite::params![bucket_id.to_string()], map_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn remove_by_bucket(bucket_id: Uuid, db: &Database) -> crate::database::Result<bool> {
        let conn = db.conn();
        let count = conn.execute(
            "DELETE FROM sync_targets WHERE bucket_id = ?1",
            rusqlite::params![bucket_id.to_string()],
        )?;
        Ok(count > 0)
    }

    pub fn list_active(db: &Database) -> crate::database::Result<Vec<SyncTarget>> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, bucket_id, target_path, last_head, last_sync, status, error_message, created_at, updated_at FROM sync_targets WHERE status = 'active' ORDER BY created_at DESC",
        )?;
        let rows = stmt
            .query_map([], map_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn update_head(id: Uuid, head_hex: &str, db: &Database) -> crate::database::Result<()> {
        let conn = db.conn();
        conn.execute(
            "UPDATE sync_targets SET last_head = ?1, last_sync = strftime('%s', 'now'), updated_at = strftime('%s', 'now') WHERE id = ?2",
            rusqlite::params![head_hex, id.to_string()],
        )?;
        Ok(())
    }

    pub fn set_status(
        id: Uuid,
        status: SyncStatus,
        error_message: Option<&str>,
        db: &Database,
    ) -> crate::database::Result<()> {
        let conn = db.conn();
        conn.execute(
            "UPDATE sync_targets SET status = ?1, error_message = ?2, updated_at = strftime('%s', 'now') WHERE id = ?3",
            rusqlite::params![status, error_message, id.to_string()],
        )?;
        Ok(())
    }
}
