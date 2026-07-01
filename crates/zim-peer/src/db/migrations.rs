use rusqlite_migration::{Migrations, M};

pub(crate) fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(
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
        ),
        // Contacts address book: nick → DID, with a `trusted` flag.
        // Trusted contacts are auto-propagated into the owner's vaults;
        // untrusted ones are shareable but opt-in per vault. `nick` is the
        // user-facing handle (PK); `did` is the canonical identity and is
        // unique so a contact can't be enrolled twice under two names.
        M::up(
            "CREATE TABLE contacts (
            nick TEXT NOT NULL PRIMARY KEY,
            did TEXT NOT NULL UNIQUE,
            trusted INTEGER NOT NULL DEFAULT 0,
            added_at INTEGER NOT NULL,
            notes TEXT
        );",
        ),
        // A contact's `via`: the relay host this contact is *reached
        // through*, mirroring `Share`'s who/where split. NULL = dialed
        // directly (a daemon); set to the hub's DID for a browser/web
        // device, so the daemon announces to the hub instead of trying to
        // dial the browser. `hub peers sync` stamps it from each device's
        // `kind`.
        M::up("ALTER TABLE contacts ADD COLUMN via TEXT;"),
    ])
}
