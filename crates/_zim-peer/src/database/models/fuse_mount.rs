use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::database::types::MountStatus;
use crate::database::Database;

/// FUSE mount configuration stored in database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuseMount {
    pub mount_id: Uuid,
    pub bucket_id: Uuid,
    pub mount_point: String,
    pub enabled: bool,
    pub auto_mount: bool,
    pub read_only: bool,
    pub cache_size_mb: i64,
    pub cache_ttl_secs: i64,
    pub status: MountStatus,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FuseMount> {
    Ok(FuseMount {
        mount_id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
        bucket_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap(),
        mount_point: row.get(2)?,
        enabled: row.get(3)?,
        auto_mount: row.get(4)?,
        read_only: row.get(5)?,
        cache_size_mb: row.get(6)?,
        cache_ttl_secs: row.get(7)?,
        status: row.get(8)?,
        error_message: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

impl FuseMount {
    /// Create a new FUSE mount configuration
    pub fn create(
        bucket_id: Uuid,
        mount_point: &str,
        auto_mount: bool,
        read_only: bool,
        cache_size_mb: Option<i64>,
        cache_ttl_secs: Option<i64>,
        db: &Database,
    ) -> crate::database::Result<FuseMount> {
        let mount_id = Uuid::new_v4();
        let cache_size = cache_size_mb.unwrap_or(100);
        let cache_ttl = cache_ttl_secs.unwrap_or(60);

        {
            let conn = db.conn();
            conn.execute(
                r#"
                INSERT INTO fuse_mounts (
                    mount_id, bucket_id, mount_point, auto_mount, read_only,
                    cache_size_mb, cache_ttl_secs
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                rusqlite::params![
                    mount_id.to_string(),
                    bucket_id.to_string(),
                    mount_point,
                    auto_mount,
                    read_only,
                    cache_size,
                    cache_ttl,
                ],
            )?;
        }

        Self::get(mount_id, db)?.ok_or_else(|| {
            crate::database::DatabaseError::Client(rusqlite::Error::QueryReturnedNoRows)
        })
    }

    /// Get a FUSE mount by ID
    pub fn get(mount_id: Uuid, db: &Database) -> crate::database::Result<Option<FuseMount>> {
        let conn = db.conn();
        let result = conn
            .query_row(
                r#"
                SELECT
                    mount_id, bucket_id, mount_point, enabled, auto_mount,
                    read_only, cache_size_mb, cache_ttl_secs, status,
                    error_message, created_at, updated_at
                FROM fuse_mounts
                WHERE mount_id = ?1
                "#,
                rusqlite::params![mount_id.to_string()],
                map_row,
            )
            .optional()?;
        Ok(result)
    }

    /// List all FUSE mounts
    pub fn list(db: &Database) -> crate::database::Result<Vec<FuseMount>> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            r#"
            SELECT
                mount_id, bucket_id, mount_point, enabled, auto_mount,
                read_only, cache_size_mb, cache_ttl_secs, status,
                error_message, created_at, updated_at
            FROM fuse_mounts
            ORDER BY created_at DESC
            "#,
        )?;
        let rows = stmt
            .query_map([], map_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Update a FUSE mount configuration
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        mount_id: Uuid,
        mount_point: Option<&str>,
        enabled: Option<bool>,
        auto_mount: Option<bool>,
        read_only: Option<bool>,
        cache_size_mb: Option<i64>,
        cache_ttl_secs: Option<i64>,
        db: &Database,
    ) -> crate::database::Result<Option<FuseMount>> {
        let existing = match Self::get(mount_id, db)? {
            Some(m) => m,
            None => return Ok(None),
        };

        let mount_point = mount_point.unwrap_or(&existing.mount_point);
        let enabled = enabled.unwrap_or(existing.enabled);
        let auto_mount = auto_mount.unwrap_or(existing.auto_mount);
        let read_only = read_only.unwrap_or(existing.read_only);
        let cache_size = cache_size_mb.unwrap_or(existing.cache_size_mb);
        let cache_ttl = cache_ttl_secs.unwrap_or(existing.cache_ttl_secs);

        {
            let conn = db.conn();
            conn.execute(
                r#"
                UPDATE fuse_mounts
                SET mount_point = ?1, enabled = ?2, auto_mount = ?3, read_only = ?4,
                    cache_size_mb = ?5, cache_ttl_secs = ?6, updated_at = CURRENT_TIMESTAMP
                WHERE mount_id = ?7
                "#,
                rusqlite::params![
                    mount_point,
                    enabled,
                    auto_mount,
                    read_only,
                    cache_size,
                    cache_ttl,
                    mount_id.to_string(),
                ],
            )?;
        }

        Self::get(mount_id, db)
    }

    /// Delete a FUSE mount
    pub fn delete(mount_id: Uuid, db: &Database) -> crate::database::Result<bool> {
        let conn = db.conn();
        let count = conn.execute(
            "DELETE FROM fuse_mounts WHERE mount_id = ?1",
            rusqlite::params![mount_id.to_string()],
        )?;
        Ok(count > 0)
    }

    /// Update the status of a FUSE mount
    pub fn update_status(
        mount_id: Uuid,
        status: MountStatus,
        error_message: Option<&str>,
        db: &Database,
    ) -> crate::database::Result<bool> {
        let conn = db.conn();
        let count = conn.execute(
            r#"
            UPDATE fuse_mounts
            SET status = ?1, error_message = ?2, updated_at = CURRENT_TIMESTAMP
            WHERE mount_id = ?3
            "#,
            rusqlite::params![status, error_message, mount_id.to_string()],
        )?;
        Ok(count > 0)
    }

    /// Get all mounts configured for auto-mount
    pub fn auto_list(db: &Database) -> crate::database::Result<Vec<FuseMount>> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            r#"
            SELECT
                mount_id, bucket_id, mount_point, enabled, auto_mount,
                read_only, cache_size_mb, cache_ttl_secs, status,
                error_message, created_at, updated_at
            FROM fuse_mounts
            WHERE auto_mount = 1 AND enabled = 1
            ORDER BY created_at ASC
            "#,
        )?;
        let rows = stmt
            .query_map([], map_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Get mounts by bucket ID
    pub fn by_bucket(bucket_id: Uuid, db: &Database) -> crate::database::Result<Vec<FuseMount>> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            r#"
            SELECT
                mount_id, bucket_id, mount_point, enabled, auto_mount,
                read_only, cache_size_mb, cache_ttl_secs, status,
                error_message, created_at, updated_at
            FROM fuse_mounts
            WHERE bucket_id = ?1
            ORDER BY created_at DESC
            "#,
        )?;
        let rows = stmt
            .query_map(rusqlite::params![bucket_id.to_string()], map_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
