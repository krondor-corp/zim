use rusqlite_migration::{Migrations, M};

pub(crate) fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(
        "CREATE TABLE vault_log (
            id TEXT NOT NULL,
            name TEXT NOT NULL,
            current TEXT NOT NULL,
            previous TEXT,
            height INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            PRIMARY KEY (id, height, current)
        );
        CREATE INDEX idx_vault_log_id_height ON vault_log(id, height);",
    )])
}
