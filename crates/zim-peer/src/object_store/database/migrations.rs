use rusqlite_migration::{Migrations, M};

pub(crate) fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(
        "CREATE TABLE blobs (
                hash TEXT PRIMARY KEY NOT NULL,
                size INTEGER NOT NULL,
                has_outboard INTEGER NOT NULL DEFAULT 0,
                state TEXT NOT NULL DEFAULT 'complete',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );",
    )])
}
