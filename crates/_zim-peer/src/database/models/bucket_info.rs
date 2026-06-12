use uuid::Uuid;

use zim_core::linked_data::{multibase::Base, Cid, Link};

use crate::database::Database;

#[derive(Debug, Clone)]
pub struct BucketInfo {
    pub id: Uuid,
    pub name: String,
    pub link: Link,
    pub created_at: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BucketLogEntry {
    pub bucket_id: Uuid,
    pub name: String,
    pub current_link: Link,
    pub previous_link: Option<Link>,
    pub height: u64,
    pub published: bool,
    pub created_at: String,
}

fn parse_link(s: &str) -> Link {
    let cid = Cid::try_from(s).expect("invalid CID in database");
    Link::from(cid)
}

fn link_to_db(link: &Link) -> String {
    Cid::from(link.clone())
        .to_string_of_base(Base::Base32Lower)
        .unwrap()
}

impl BucketInfo {
    pub fn get(id: &Uuid, db: &Database) -> crate::database::Result<Option<BucketInfo>> {
        let conn = db.conn();
        let result = conn
            .query_row(
                "SELECT bucket_id, name, current_link, created_at
                 FROM bucket_log
                 WHERE bucket_id = ?1
                 ORDER BY height DESC
                 LIMIT 1",
                rusqlite::params![id.to_string()],
                |row| {
                    let bucket_id: String = row.get(0)?;
                    let current_link: String = row.get(2)?;
                    Ok(BucketInfo {
                        id: Uuid::parse_str(&bucket_id).unwrap(),
                        name: row.get(1)?,
                        link: parse_link(&current_link),
                        created_at: row.get(3)?,
                    })
                },
            )
            .optional()?;
        Ok(result)
    }

    pub fn list(
        prefix: Option<String>,
        limit: Option<u32>,
        db: &Database,
    ) -> crate::database::Result<Vec<BucketInfo>> {
        let conn = db.conn();
        let limit_val = limit.unwrap_or(100).min(1000) as i64;
        let pattern = prefix
            .map(|p| format!("{p}%"))
            .unwrap_or_else(|| "%".to_string());

        let mut stmt = conn.prepare(
            "SELECT
                bl.bucket_id,
                bl.name,
                bl.current_link,
                MIN(bl.created_at) as created_at
            FROM bucket_log bl
            INNER JOIN (
                SELECT bucket_id, MAX(height) as max_height
                FROM bucket_log
                GROUP BY bucket_id
            ) latest ON bl.bucket_id = latest.bucket_id AND bl.height = latest.max_height
            WHERE bl.name LIKE ?1
            GROUP BY bl.bucket_id, bl.name, bl.current_link
            ORDER BY created_at DESC
            LIMIT ?2",
        )?;

        let rows = stmt
            .query_map(rusqlite::params![pattern, limit_val], |row| {
                let bucket_id: String = row.get(0)?;
                let current_link: String = row.get(2)?;
                Ok(BucketInfo {
                    id: Uuid::parse_str(&bucket_id).unwrap(),
                    name: row.get(1)?,
                    link: parse_link(&current_link),
                    created_at: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(rows)
    }
}

impl BucketLogEntry {
    pub fn get_page(
        bucket_id: &Uuid,
        page: u32,
        page_size: u32,
        db: &Database,
    ) -> crate::database::Result<Vec<BucketLogEntry>> {
        let conn = db.conn();
        let limit = page_size.min(100) as i64;
        let offset = (page * page_size) as i64;

        let mut stmt = conn.prepare(
            "SELECT bucket_id, name, current_link, previous_link, height, published, created_at
             FROM bucket_log
             WHERE bucket_id = ?1
             ORDER BY height DESC
             LIMIT ?2 OFFSET ?3",
        )?;

        let rows = stmt
            .query_map(
                rusqlite::params![bucket_id.to_string(), limit, offset],
                map_log_row,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(rows)
    }

    pub fn get_all(
        bucket_id: &Uuid,
        db: &Database,
    ) -> crate::database::Result<Vec<BucketLogEntry>> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT bucket_id, name, current_link, previous_link, height, published, created_at
             FROM bucket_log
             WHERE bucket_id = ?1
             ORDER BY height ASC",
        )?;

        let rows = stmt
            .query_map(rusqlite::params![bucket_id.to_string()], map_log_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(rows)
    }

    #[allow(dead_code)]
    pub fn count(bucket_id: &Uuid, db: &Database) -> crate::database::Result<i64> {
        let conn = db.conn();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM bucket_log WHERE bucket_id = ?1",
            rusqlite::params![bucket_id.to_string()],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn append(
        bucket_id: &Uuid,
        name: &str,
        current: &Link,
        previous: Option<&Link>,
        height: u64,
        published: bool,
        db: &Database,
    ) -> crate::database::Result<()> {
        let conn = db.conn();
        let current_str = link_to_db(current);
        let previous_str = previous.map(link_to_db);
        conn.execute(
            "INSERT INTO bucket_log (bucket_id, name, current_link, previous_link, height, published, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, CURRENT_TIMESTAMP)",
            rusqlite::params![
                bucket_id.to_string(),
                name,
                current_str,
                previous_str,
                height as i64,
                published,
            ],
        )?;
        Ok(())
    }

    pub fn exists_at(
        bucket_id: &Uuid,
        link: &Link,
        height: u64,
        db: &Database,
    ) -> crate::database::Result<bool> {
        let conn = db.conn();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM bucket_log
             WHERE bucket_id = ?1 AND current_link = ?2 AND height = ?3",
            rusqlite::params![bucket_id.to_string(), link_to_db(link), height as i64,],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn heights_for_link(
        bucket_id: &Uuid,
        link: &Link,
        db: &Database,
    ) -> crate::database::Result<Vec<u64>> {
        let conn = db.conn();
        let mut stmt = conn
            .prepare("SELECT height FROM bucket_log WHERE bucket_id = ?1 AND current_link = ?2")?;
        let rows = stmt
            .query_map(
                rusqlite::params![bucket_id.to_string(), link_to_db(link)],
                |row| {
                    let h: i64 = row.get(0)?;
                    Ok(h as u64)
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn max_height(bucket_id: &Uuid, db: &Database) -> crate::database::Result<Option<u64>> {
        let conn = db.conn();
        let result: Option<i64> = conn
            .query_row(
                "SELECT MAX(height) FROM bucket_log WHERE bucket_id = ?1",
                rusqlite::params![bucket_id.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(result.map(|h| h as u64))
    }

    pub fn heads_at(
        bucket_id: &Uuid,
        height: u64,
        db: &Database,
    ) -> crate::database::Result<Vec<Link>> {
        let conn = db.conn();
        let mut stmt = conn
            .prepare("SELECT current_link FROM bucket_log WHERE bucket_id = ?1 AND height = ?2")?;
        let rows = stmt
            .query_map(
                rusqlite::params![bucket_id.to_string(), height as i64],
                |row| {
                    let s: String = row.get(0)?;
                    Ok(parse_link(&s))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn latest_published(
        bucket_id: &Uuid,
        db: &Database,
    ) -> crate::database::Result<Option<(Link, u64)>> {
        let conn = db.conn();
        let result = conn
            .query_row(
                "SELECT current_link, height FROM bucket_log
                 WHERE bucket_id = ?1 AND published = TRUE
                 ORDER BY height DESC LIMIT 1",
                rusqlite::params![bucket_id.to_string()],
                |row| {
                    let link_str: String = row.get(0)?;
                    let h: i64 = row.get(1)?;
                    Ok((parse_link(&link_str), h as u64))
                },
            )
            .optional()?;
        Ok(result)
    }

    pub fn list_bucket_ids(db: &Database) -> crate::database::Result<Vec<Uuid>> {
        let conn = db.conn();
        let mut stmt =
            conn.prepare("SELECT DISTINCT bucket_id FROM bucket_log ORDER BY bucket_id")?;
        let rows = stmt
            .query_map([], |row| {
                let s: String = row.get(0)?;
                Ok(Uuid::parse_str(&s).unwrap())
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn list_syncable_bucket_ids(db: &Database) -> crate::database::Result<Vec<Uuid>> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT bl.bucket_id
             FROM bucket_log bl
             LEFT JOIN bucket_status bs ON bl.bucket_id = bs.bucket_id
             WHERE bs.status IS NULL OR bs.status = 'active'
             ORDER BY bl.bucket_id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let s: String = row.get(0)?;
                Ok(Uuid::parse_str(&s).unwrap())
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn bucket_exists(bucket_id: &Uuid, db: &Database) -> crate::database::Result<bool> {
        let conn = db.conn();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM bucket_log WHERE bucket_id = ?1",
            rusqlite::params![bucket_id.to_string()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}

fn map_log_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BucketLogEntry> {
    let bucket_id: String = row.get(0)?;
    let current_link: String = row.get(2)?;
    let previous_link: Option<String> = row.get(3)?;
    Ok(BucketLogEntry {
        bucket_id: Uuid::parse_str(&bucket_id).unwrap(),
        name: row.get(1)?,
        current_link: parse_link(&current_link),
        previous_link: previous_link.map(|s| parse_link(&s)),
        height: row.get::<_, i64>(4)? as u64,
        published: row.get(5)?,
        created_at: row.get(6)?,
    })
}

use rusqlite::OptionalExtension;
