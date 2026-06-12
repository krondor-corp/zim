use rusqlite::{params, OptionalExtension};
use zim_core::vault::VaultId;

use super::client::Database;
use super::error::Result;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct LogEntry {
    pub id: VaultId,
    pub name: String,
    pub current: String,
    pub previous: Option<String>,
    pub height: u64,
    pub created_at: i64,
}

impl LogEntry {
    pub fn append(
        db: &Database,
        id: VaultId,
        name: &str,
        current: &str,
        previous: Option<&str>,
        height: u64,
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let conn = db.conn();
        // `INSERT OR IGNORE`: re-appending the same `(id, height,
        // current)` tuple silently no-ops. That keeps `apply_chain`
        // idempotent under concurrent pulls — two coordinator tasks
        // walking the same remote chain don't trip a UNIQUE constraint
        // on their second insert. Different `current` at the same
        // height is still a distinct row (forks are preserved).
        conn.execute(
            "INSERT OR IGNORE INTO vault_log (id, name, current, previous, height, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id.to_string(), name, current, previous, height as i64, now],
        )?;
        Ok(())
    }

    pub fn exists(db: &Database, id: VaultId) -> Result<bool> {
        let conn = db.conn();
        let result = conn
            .query_row(
                "SELECT 1 FROM vault_log WHERE id = ?1 LIMIT 1",
                params![id.to_string()],
                |_| Ok(()),
            )
            .optional()?;
        Ok(result.is_some())
    }

    pub fn heads(db: &Database, id: VaultId, height: u64) -> Result<Vec<String>> {
        let conn = db.conn();
        let mut stmt =
            conn.prepare("SELECT current FROM vault_log WHERE id = ?1 AND height = ?2")?;
        let rows = stmt
            .query_map(params![id.to_string(), height as i64], |row| row.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        Ok(rows)
    }

    pub fn height(db: &Database, id: VaultId) -> Result<Option<u64>> {
        let conn = db.conn();
        let result: Option<Option<i64>> = conn
            .query_row(
                "SELECT MAX(height) FROM vault_log WHERE id = ?1",
                params![id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(result.flatten().map(|h| h as u64))
    }

    pub fn has(db: &Database, id: VaultId, current: &str) -> Result<Vec<u64>> {
        let conn = db.conn();
        let mut stmt =
            conn.prepare("SELECT height FROM vault_log WHERE id = ?1 AND current = ?2")?;
        let rows = stmt
            .query_map(params![id.to_string(), current], |row| {
                row.get::<_, i64>(0).map(|h| h as u64)
            })?
            .collect::<std::result::Result<Vec<u64>, _>>()?;
        Ok(rows)
    }

    pub fn list_vault_ids(db: &Database) -> Result<Vec<VaultId>> {
        let conn = db.conn();
        let mut stmt = conn.prepare("SELECT DISTINCT id FROM vault_log ORDER BY id")?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        let mut ids = Vec::with_capacity(rows.len());
        for s in rows {
            if let Ok(id) = s.parse::<VaultId>() {
                ids.push(id);
            }
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_id(byte: u8) -> VaultId {
        VaultId::from_hash(zim_core::linked_data::Hash::new([byte; 32]))
    }

    #[test]
    fn test_append_and_query() {
        let db = Database::in_memory().unwrap();
        let id = test_id(7);

        LogEntry::append(&db, id, "my-vault", "link0", None, 0).unwrap();
        assert!(LogEntry::exists(&db, id).unwrap());
        assert_eq!(LogEntry::height(&db, id).unwrap(), Some(0));

        let heads = LogEntry::heads(&db, id, 0).unwrap();
        assert_eq!(heads, vec!["link0"]);

        let heights = LogEntry::has(&db, id, "link0").unwrap();
        assert_eq!(heights, vec![0]);
    }

    #[test]
    fn test_append_chain() {
        let db = Database::in_memory().unwrap();
        let id = test_id(7);

        LogEntry::append(&db, id, "v", "link0", None, 0).unwrap();
        LogEntry::append(&db, id, "v", "link1", Some("link0"), 1).unwrap();
        LogEntry::append(&db, id, "v", "link2", Some("link1"), 2).unwrap();

        assert_eq!(LogEntry::height(&db, id).unwrap(), Some(2));
        assert_eq!(LogEntry::heads(&db, id, 2).unwrap(), vec!["link2"]);
    }

    #[test]
    fn test_list_vault_ids() {
        let db = Database::in_memory().unwrap();
        let id1 = test_id(1);
        let id2 = test_id(2);

        LogEntry::append(&db, id1, "a", "link0", None, 0).unwrap();
        LogEntry::append(&db, id2, "b", "link0", None, 0).unwrap();

        let ids = LogEntry::list_vault_ids(&db).unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
    }

    #[test]
    fn test_nonexistent() {
        let db = Database::in_memory().unwrap();
        let id = test_id(7);
        assert!(!LogEntry::exists(&db, id).unwrap());
        assert_eq!(LogEntry::height(&db, id).unwrap(), None);
    }
}
