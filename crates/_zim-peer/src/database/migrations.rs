use rusqlite_migration::{Migrations, M};

pub(crate) fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(
            "CREATE TABLE buckets (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                link VARCHAR(255) NOT NULL,
                created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE UNIQUE INDEX buckets_id_name ON buckets (id, name);",
        ),
        M::up(
            "ALTER TABLE buckets ADD COLUMN sync_status TEXT NOT NULL DEFAULT 'synced';
            ALTER TABLE buckets ADD COLUMN last_sync_attempt TIMESTAMP;
            ALTER TABLE buckets ADD COLUMN sync_error TEXT;",
        ),
        M::up(
            "CREATE TABLE bucket_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                bucket_id TEXT NOT NULL,
                name TEXT NOT NULL,
                current_link VARCHAR(255) NOT NULL,
                previous_link VARCHAR(255),
                height INTEGER NOT NULL,
                created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(bucket_id, height),
                UNIQUE(bucket_id, current_link)
            );
            CREATE INDEX idx_bucket_log_bucket_height ON bucket_log(bucket_id, height DESC);
            CREATE INDEX idx_bucket_log_bucket_link ON bucket_log(bucket_id, current_link);",
        ),
        M::up("DROP TABLE buckets;"),
        M::up(
            "ALTER TABLE bucket_log ADD COLUMN published BOOLEAN NOT NULL DEFAULT FALSE;
            CREATE INDEX idx_bucket_log_bucket_published ON bucket_log(bucket_id, published, height DESC);",
        ),
        M::up(
            "CREATE TABLE fuse_mounts (
                mount_id TEXT PRIMARY KEY,
                bucket_id TEXT NOT NULL,
                mount_point TEXT NOT NULL UNIQUE,
                enabled INTEGER NOT NULL DEFAULT 1,
                auto_mount INTEGER NOT NULL DEFAULT 0,
                read_only INTEGER NOT NULL DEFAULT 0,
                cache_size_mb INTEGER NOT NULL DEFAULT 100,
                cache_ttl_secs INTEGER NOT NULL DEFAULT 60,
                status TEXT NOT NULL DEFAULT 'stopped',
                error_message TEXT,
                created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX idx_fuse_mounts_bucket_id ON fuse_mounts(bucket_id);
            CREATE INDEX idx_fuse_mounts_auto_mount ON fuse_mounts(auto_mount) WHERE auto_mount = 1 AND enabled = 1;",
        ),
        M::up(
            "CREATE TABLE bucket_status (
                bucket_id TEXT PRIMARY KEY,
                status TEXT NOT NULL DEFAULT 'pending',
                shared_by TEXT,
                updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX idx_bucket_status_status ON bucket_status(status);",
        ),
        M::up(
            "CREATE TABLE gateway_cache (
                bucket_id        TEXT    NOT NULL,
                height           INTEGER NOT NULL,
                path             TEXT    NOT NULL,
                query_string     TEXT    NOT NULL DEFAULT '',
                link             TEXT    NOT NULL,
                content_size     INTEGER NOT NULL DEFAULT 0,
                mime_type        TEXT    NOT NULL DEFAULT 'application/octet-stream',
                created_at       INTEGER NOT NULL DEFAULT (unixepoch()),
                last_accessed    INTEGER NOT NULL DEFAULT (unixepoch()),
                PRIMARY KEY (bucket_id, height, path, query_string)
            );
            CREATE INDEX idx_cache_bucket_height ON gateway_cache (bucket_id, height);
            CREATE INDEX idx_cache_last_accessed ON gateway_cache (last_accessed);",
        ),
        M::up(
            "CREATE TABLE sync_targets (
                id          TEXT PRIMARY KEY,
                bucket_id   TEXT NOT NULL,
                target_path TEXT NOT NULL UNIQUE,
                last_head   TEXT,
                last_sync   INTEGER,
                status      TEXT NOT NULL DEFAULT 'active',
                error_message TEXT,
                created_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                updated_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            );
            CREATE INDEX idx_sync_targets_bucket_id ON sync_targets(bucket_id);
            CREATE INDEX idx_sync_targets_active ON sync_targets(status) WHERE status = 'active';",
        ),
    ])
}
