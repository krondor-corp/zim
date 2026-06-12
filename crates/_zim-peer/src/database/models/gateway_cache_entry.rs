use std::path::Path;

use rusqlite::{params, OptionalExtension};
use uuid::Uuid;

use mime::Mime;
use zim_core::linked_data::Hash;

use crate::database::Database;

/// A cached gateway response lookup result.
#[derive(Debug, Clone)]
pub struct GatewayCacheEntry {
    /// BLAKE3 hash of the cached content (content-addressed link).
    pub link: Hash,
    /// MIME type of the cached content.
    pub mime_type: Mime,
}

impl GatewayCacheEntry {
    /// Look up a cached entry by its key.
    pub fn lookup(
        bucket_id: &Uuid,
        height: u64,
        path: &Path,
        query_string: Option<&str>,
        db: &Database,
    ) -> crate::database::Result<Option<Self>> {
        let conn = db.conn();
        let bid = bucket_id.to_string();
        let path_str = path.to_string_lossy();
        let qs = query_string.unwrap_or("");

        let entry: Option<Self> = conn
            .query_row(
                "SELECT link, mime_type FROM gateway_cache
                 WHERE bucket_id = ?1 AND height = ?2 AND path = ?3 AND query_string = ?4",
                params![bid, height as i64, path_str.as_ref(), qs],
                |row| {
                    let link_str: String = row.get(0)?;
                    let mime_str: String = row.get(1)?;
                    Ok(GatewayCacheEntry {
                        link: link_str.parse::<Hash>().unwrap(),
                        mime_type: mime_str.parse::<Mime>().unwrap(),
                    })
                },
            )
            .optional()?;

        // Touch last_accessed on hit
        if entry.is_some() {
            let _ = conn.execute(
                "UPDATE gateway_cache SET last_accessed = unixepoch()
                 WHERE bucket_id = ?1 AND height = ?2 AND path = ?3 AND query_string = ?4",
                params![bid, height as i64, path_str.as_ref(), qs],
            );
        }

        Ok(entry)
    }

    /// Append a cache entry to the log. No-op if the key already exists.
    #[allow(clippy::too_many_arguments)]
    pub fn log(
        bucket_id: &Uuid,
        height: u64,
        path: &Path,
        query_string: Option<&str>,
        link: &Hash,
        content_size: u64,
        mime_type: &Mime,
        db: &Database,
    ) -> crate::database::Result<()> {
        let conn = db.conn();
        conn.execute(
            "INSERT OR IGNORE INTO gateway_cache
             (bucket_id, height, path, query_string, link, content_size, mime_type,
              created_at, last_accessed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, unixepoch(), unixepoch())",
            params![
                bucket_id.to_string(),
                height as i64,
                path.to_string_lossy().as_ref(),
                query_string.unwrap_or(""),
                link.to_string(),
                content_size as i64,
                mime_type.to_string(),
            ],
        )?;
        Ok(())
    }

    /// Remove entries for old heights across all buckets, keeping only the most recent `keep_versions` each.
    pub fn evict_old_heights(keep_versions: u32, db: &Database) -> crate::database::Result<u64> {
        let conn = db.conn();
        let count = conn.execute(
            "DELETE FROM gateway_cache
             WHERE rowid IN (
                 SELECT gc.rowid FROM gateway_cache gc
                 WHERE gc.height < (
                     SELECT COALESCE(MIN(h), 0) FROM (
                         SELECT DISTINCT height AS h FROM gateway_cache gc2
                         WHERE gc2.bucket_id = gc.bucket_id
                         ORDER BY height DESC
                         LIMIT ?1
                     )
                 )
             )",
            params![keep_versions],
        )?;
        Ok(count as u64)
    }

    /// Remove LRU entries until total size is under the limit.
    /// Returns the links of removed entries.
    pub fn evict_lru(max_total_size: u64, db: &Database) -> crate::database::Result<Vec<String>> {
        let conn = db.conn();
        let tx = conn.unchecked_transaction()?;

        let total: i64 = tx.query_row(
            "SELECT COALESCE(SUM(content_size), 0) FROM gateway_cache",
            [],
            |row| row.get(0),
        )?;

        if total <= max_total_size as i64 {
            return Ok(Vec::new());
        }

        let to_free = total - max_total_size as i64;
        let mut freed: i64 = 0;
        let mut removed_links = Vec::new();

        let mut stmt = tx.prepare(
            "SELECT rowid, link, content_size FROM gateway_cache ORDER BY last_accessed ASC",
        )?;
        let rows: Vec<(i64, String, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);

        for (rowid, link, size) in rows {
            if freed >= to_free {
                break;
            }

            tx.execute("DELETE FROM gateway_cache WHERE rowid = ?1", params![rowid])?;

            freed += size;
            removed_links.push(link);
        }

        tx.commit()?;
        Ok(removed_links)
    }

    /// Remove entries older than `max_age_secs`.
    /// Returns the links of removed entries.
    pub fn evict_expired(max_age_secs: u64, db: &Database) -> crate::database::Result<Vec<String>> {
        let conn = db.conn();

        let mut stmt = conn.prepare(
            "SELECT DISTINCT link FROM gateway_cache
             WHERE created_at < unixepoch() - ?1",
        )?;
        let links: Vec<String> = stmt
            .query_map(params![max_age_secs as i64], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);

        conn.execute(
            "DELETE FROM gateway_cache WHERE created_at < unixepoch() - ?1",
            params![max_age_secs as i64],
        )?;

        Ok(links)
    }

    /// Get all links still referenced in the index.
    pub fn referenced_links(db: &Database) -> crate::database::Result<Vec<String>> {
        let conn = db.conn();
        let mut stmt = conn.prepare("SELECT DISTINCT link FROM gateway_cache")?;
        let links: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(links)
    }

    /// Count total entries (for tests).
    #[cfg(test)]
    pub fn count(db: &Database) -> crate::database::Result<i64> {
        let conn = db.conn();
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM gateway_cache", [], |row| row.get(0))?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_lookup() {
        let db = Database::memory().unwrap();
        let bucket = Uuid::new_v4();
        let link = Hash::new(b"test content");
        let mime = mime::IMAGE_JPEG;

        // Miss
        assert!(
            GatewayCacheEntry::lookup(&bucket, 1, Path::new("/photo.jpg"), None, &db)
                .unwrap()
                .is_none()
        );

        // Insert and hit
        GatewayCacheEntry::log(
            &bucket,
            1,
            Path::new("/photo.jpg"),
            None,
            &link,
            1024,
            &mime,
            &db,
        )
        .unwrap();

        let entry = GatewayCacheEntry::lookup(&bucket, 1, Path::new("/photo.jpg"), None, &db)
            .unwrap()
            .unwrap();
        assert_eq!(entry.link, link);
        assert_eq!(entry.mime_type, mime::IMAGE_JPEG);
    }

    #[test]
    fn test_query_string_differentiates_entries() {
        let db = Database::memory().unwrap();
        let bucket = Uuid::new_v4();
        let link_orig = Hash::new(b"original");
        let link_thumb = Hash::new(b"thumbnail");
        let mime = mime::IMAGE_JPEG;

        GatewayCacheEntry::log(
            &bucket,
            1,
            Path::new("/photo.jpg"),
            None,
            &link_orig,
            5000,
            &mime,
            &db,
        )
        .unwrap();
        GatewayCacheEntry::log(
            &bucket,
            1,
            Path::new("/photo.jpg"),
            Some("w=200"),
            &link_thumb,
            500,
            &mime,
            &db,
        )
        .unwrap();

        let original = GatewayCacheEntry::lookup(&bucket, 1, Path::new("/photo.jpg"), None, &db)
            .unwrap()
            .unwrap();
        assert_eq!(original.link, link_orig);

        let thumb =
            GatewayCacheEntry::lookup(&bucket, 1, Path::new("/photo.jpg"), Some("w=200"), &db)
                .unwrap()
                .unwrap();
        assert_eq!(thumb.link, link_thumb);
    }

    #[test]
    fn test_evict_old_heights() {
        let db = Database::memory().unwrap();
        let bucket = Uuid::new_v4();

        for h in 1u64..=3 {
            let link = Hash::new(format!("v{}", h).as_bytes());
            GatewayCacheEntry::log(
                &bucket,
                h,
                Path::new("/file.txt"),
                None,
                &link,
                100,
                &mime::TEXT_PLAIN,
                &db,
            )
            .unwrap();
        }
        assert_eq!(GatewayCacheEntry::count(&db).unwrap(), 3);

        let removed = GatewayCacheEntry::evict_old_heights(1, &db).unwrap();
        assert_eq!(removed, 2);
        assert_eq!(GatewayCacheEntry::count(&db).unwrap(), 1);

        assert!(
            GatewayCacheEntry::lookup(&bucket, 3, Path::new("/file.txt"), None, &db)
                .unwrap()
                .is_some()
        );
        assert!(
            GatewayCacheEntry::lookup(&bucket, 1, Path::new("/file.txt"), None, &db)
                .unwrap()
                .is_none()
        );
    }
}
