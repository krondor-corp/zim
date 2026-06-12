-- Create sync_targets table for daemon-managed filesystem backup sync (T-018).
-- Pattern mirrors fuse_mounts: one row per registered backup target.
CREATE TABLE sync_targets (
    id          TEXT PRIMARY KEY,           -- uuid
    bucket_id   TEXT NOT NULL,
    target_path TEXT NOT NULL UNIQUE,       -- real filesystem path chosen by user
    last_head   TEXT,                       -- Link hex of last-synced manifest head (null = never synced)
    last_sync   INTEGER,                   -- unix seconds of last successful sync
    status      TEXT NOT NULL DEFAULT 'active',  -- active | paused | error
    error_message TEXT,                    -- populated when status = 'error'
    created_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX idx_sync_targets_bucket_id ON sync_targets(bucket_id);
CREATE INDEX idx_sync_targets_active ON sync_targets(status) WHERE status = 'active';
