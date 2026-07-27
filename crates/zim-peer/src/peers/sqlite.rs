//! SQLite-backed [`PeerStore`] — the daemon's contacts address book.
//!
//! Lives in the same `log.sqlite` as the vault log (one DB, one
//! migration set; see [`crate::db`]). Replaces the old
//! `peers.toml`-backed store: a contact is a `nick → DID` row with a
//! `trusted` flag. Cheap to open standalone (the CLI does this for
//! `hub peers sync`) — opening runs migrations idempotently, so the
//! table exists whether or not the daemon has started.

use async_trait::async_trait;
use rusqlite::{params, OptionalExtension};
use zim_did::Did;

use std::path::Path;

use crate::db::{Database, DatabaseError};
use crate::peers::{PeerEntry, PeerStore, PeerStoreError};

type StoreError = PeerStoreError<DatabaseError>;

#[derive(Debug, Clone)]
pub struct SqlitePeerStore {
    db: Database,
}

impl SqlitePeerStore {
    /// Open (and migrate) the contacts book at `path`. The daemon and
    /// the CLI both call this against the same `log.sqlite`; WAL mode
    /// handles the concurrent access.
    pub fn open(path: &Path) -> Result<Self, DatabaseError> {
        Ok(Self {
            db: Database::new(path)?,
        })
    }

    /// In-memory book for tests.
    pub fn in_memory() -> Result<Self, DatabaseError> {
        Ok(Self {
            db: Database::in_memory()?,
        })
    }

    /// Insert or replace a contact, stamping its `via` (the relay host it
    /// is reached through). The trait [`upsert`](PeerStore::upsert)
    /// delegates here with `via = None`; `hub peers sync` calls this
    /// directly with the hub for `kind = web` devices.
    ///
    /// `added_at` survives a re-add (the `ON CONFLICT` branch leaves it);
    /// `notes` is preserved when the new value is `NULL`. A DID already
    /// enrolled under a *different* nick trips the `did UNIQUE`
    /// constraint, surfacing as a backend error — one contact, one name.
    pub async fn upsert_via(
        &self,
        nick: &str,
        identity: Did,
        via: Option<Did>,
        trusted: bool,
        notes: Option<String>,
    ) -> Result<(), StoreError> {
        let now = chrono::Utc::now().timestamp();
        let did = identity.to_string();
        let via = via.map(|v| v.to_string());
        let conn = self.db.conn();
        conn.execute(
            "INSERT INTO contacts (nick, did, trusted, added_at, notes, via)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(nick) DO UPDATE SET
                 did = excluded.did,
                 trusted = excluded.trusted,
                 notes = COALESCE(excluded.notes, contacts.notes),
                 via = excluded.via",
            params![nick, did, trusted, now, notes, via],
        )
        .map_err(backend)?;
        Ok(())
    }
}

/// Columns selected for every contact, in order — kept in one place so
/// the row mapper and the three `SELECT`s can't drift.
const COLS: &str = "nick, did, trusted, added_at, notes, via";

/// A raw contact row in `COLS` order: `(nick, did, trusted, added_at,
/// notes, via)`. Fed to [`row_to_entry`].
type ContactRow = (String, String, bool, i64, Option<String>, Option<String>);

/// Map a `(nick, did, trusted, added_at, notes, via)` row into a
/// [`PeerEntry`], parsing the stored DID strings back into
/// [`Identity`]s. A row whose DID/via no longer parses is a corrupt
/// write, surfaced as a backend error rather than silently skipped.
fn row_to_entry(
    nick: String,
    did: String,
    trusted: bool,
    added_at: i64,
    notes: Option<String>,
    via: Option<String>,
) -> Result<PeerEntry, StoreError> {
    let parse = |s: &str| -> Result<Did, StoreError> {
        Did::parse(s).map_err(|e| {
            PeerStoreError::Backend(DatabaseError::Deserialize(anyhow::anyhow!(
                "contact `{nick}` has unparseable DID `{s}`: {e}"
            )))
        })
    };
    let identity = parse(&did)?;
    let via = via.as_deref().map(parse).transpose()?;
    Ok(PeerEntry {
        nick,
        identity,
        via,
        trusted,
        added_at,
        notes,
    })
}

/// Read every column of a contact row into the tuple `row_to_entry`
/// wants. Shared by `list`, `get`, and `list_trusted`.
fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ContactRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

#[async_trait]
impl PeerStore for SqlitePeerStore {
    type Error = DatabaseError;

    async fn list(&self) -> Result<Vec<PeerEntry>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn
            .prepare(&format!("SELECT {COLS} FROM contacts ORDER BY added_at"))
            .map_err(backend)?;
        let rows = stmt
            .query_map([], read_row)
            .map_err(backend)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(backend)?;
        rows.into_iter()
            .map(|(nick, did, trusted, added_at, notes, via)| {
                row_to_entry(nick, did, trusted, added_at, notes, via)
            })
            .collect()
    }

    async fn get(&self, nick: &str) -> Result<Option<PeerEntry>, StoreError> {
        let conn = self.db.conn();
        let row = conn
            .query_row(
                &format!("SELECT {COLS} FROM contacts WHERE nick = ?1"),
                params![nick],
                read_row,
            )
            .optional()
            .map_err(backend)?;
        match row {
            Some((nick, did, trusted, added_at, notes, via)) => Ok(Some(row_to_entry(
                nick, did, trusted, added_at, notes, via,
            )?)),
            None => Ok(None),
        }
    }

    async fn upsert(
        &self,
        nick: &str,
        identity: Did,
        trusted: bool,
        notes: Option<String>,
    ) -> Result<(), StoreError> {
        // Direct contact (no relay). For a hosted (browser) contact, the
        // caller uses [`Self::upsert_via`] to stamp the hub.
        self.upsert_via(nick, identity, None, trusted, notes).await
    }

    async fn remove(&self, nick: &str) -> Result<PeerEntry, StoreError> {
        let existing = self
            .get(nick)
            .await?
            .ok_or_else(|| PeerStoreError::NotFound(nick.to_string()))?;
        let conn = self.db.conn();
        conn.execute("DELETE FROM contacts WHERE nick = ?1", params![nick])
            .map_err(backend)?;
        Ok(existing)
    }

    async fn set_trusted(&self, nick: &str, trusted: bool) -> Result<(), StoreError> {
        let conn = self.db.conn();
        let changed = conn
            .execute(
                "UPDATE contacts SET trusted = ?1 WHERE nick = ?2",
                params![trusted, nick],
            )
            .map_err(backend)?;
        if changed == 0 {
            return Err(PeerStoreError::NotFound(nick.to_string()));
        }
        Ok(())
    }

    async fn list_trusted(&self) -> Result<Vec<PeerEntry>, StoreError> {
        let conn = self.db.conn();
        let mut stmt = conn
            .prepare(&format!(
                "SELECT {COLS} FROM contacts WHERE trusted = 1 ORDER BY added_at"
            ))
            .map_err(backend)?;
        let rows = stmt
            .query_map([], read_row)
            .map_err(backend)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(backend)?;
        rows.into_iter()
            .map(|(nick, did, trusted, added_at, notes, via)| {
                row_to_entry(nick, did, trusted, added_at, notes, via)
            })
            .collect()
    }
}

fn backend(e: rusqlite::Error) -> StoreError {
    PeerStoreError::Backend(DatabaseError::Client(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn did_key() -> Did {
        // A deterministic did:key for a fixed pubkey.
        let pk = zim_crypto::PrivateKey::generate().public();
        Did::from_key(&pk)
    }

    #[tokio::test]
    async fn upsert_then_get_roundtrips_and_preserves_added_at() {
        let store = SqlitePeerStore::in_memory().unwrap();
        let id = did_key();

        store
            .upsert("laptop", id.clone(), true, Some("my macbook".into()))
            .await
            .unwrap();
        let first = store.get("laptop").await.unwrap().unwrap();
        assert!(first.trusted);
        assert_eq!(first.identity, id);
        assert_eq!(first.notes.as_deref(), Some("my macbook"));

        // Re-add with notes=None: notes survive, added_at sticks, trust flips.
        store
            .upsert("laptop", id.clone(), false, None)
            .await
            .unwrap();
        let second = store.get("laptop").await.unwrap().unwrap();
        assert!(!second.trusted);
        assert_eq!(second.notes.as_deref(), Some("my macbook"));
        assert_eq!(second.added_at, first.added_at);
    }

    #[tokio::test]
    async fn upsert_via_roundtrips_the_relay_host() {
        let store = SqlitePeerStore::in_memory().unwrap();
        let browser = did_key();
        let hub = did_key();

        // A web device is reached via the hub.
        store
            .upsert_via("browser", browser.clone(), Some(hub.clone()), true, None)
            .await
            .unwrap();
        let entry = store.get("browser").await.unwrap().unwrap();
        assert_eq!(entry.identity, browser);
        assert_eq!(entry.via.as_ref(), Some(&hub), "via host round-trips");

        // A plain `upsert` (the trait path) is a direct contact — no via.
        store.upsert("daemon", did_key(), true, None).await.unwrap();
        assert!(store.get("daemon").await.unwrap().unwrap().via.is_none());

        // list_trusted carries via too.
        let trusted = store.list_trusted().await.unwrap();
        let b = trusted.iter().find(|e| e.nick == "browser").unwrap();
        assert_eq!(b.via.as_ref(), Some(&hub));
    }

    #[tokio::test]
    async fn set_trusted_and_list_trusted_filter() {
        let store = SqlitePeerStore::in_memory().unwrap();
        store.upsert("a", did_key(), false, None).await.unwrap();
        store.upsert("b", did_key(), true, None).await.unwrap();

        assert_eq!(store.list_trusted().await.unwrap().len(), 1);
        store.set_trusted("a", true).await.unwrap();
        assert_eq!(store.list_trusted().await.unwrap().len(), 2);

        assert!(matches!(
            store.set_trusted("missing", true).await,
            Err(PeerStoreError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn remove_returns_entry_then_absent() {
        let store = SqlitePeerStore::in_memory().unwrap();
        store.upsert("gone", did_key(), false, None).await.unwrap();
        let removed = store.remove("gone").await.unwrap();
        assert_eq!(removed.nick, "gone");
        assert!(store.get("gone").await.unwrap().is_none());
        assert!(matches!(
            store.remove("gone").await,
            Err(PeerStoreError::NotFound(_))
        ));
    }
}
