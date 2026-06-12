-- App-level user table for the hub's multi-tenant gate.
--
-- One row per verified Google login. The role model is two
-- independent boolean flags:
--   is_admin       — can read /_admin + change other users' roles.
--   is_authorized  — can use the workspace UI/API.
-- The pending state is "row exists, both flags false."
-- Bootstrapping the first admin: emails configured in
-- `config.auth.admin_emails` are inserted with both flags `true`
-- by the OAuth callback on first sign-in.

CREATE TABLE users (
    id              TEXT PRIMARY KEY NOT NULL,
    email           TEXT NOT NULL UNIQUE,
    name            TEXT NOT NULL DEFAULT '',
    is_admin        INTEGER NOT NULL DEFAULT 0,
    is_authorized   INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
