-- One-time handoff tokens for the native app's sign-in.
--
-- Lifecycle:
-- 1. The app opens the provider's sign-in in a system web-auth session
--    with `mobile_challenge=<base64url SHA-256 of a verifier it keeps>`.
-- 2. When the provider handshake succeeds, the hub inserts a row here
--    (instead of setting a cookie in the system browser) and redirects
--    to `zim-auth://callback?token=…`.
-- 3. The app loads `/auth/mobile/complete?token=…&verifier=…` in its own
--    webview. The hub deletes the row, checks the verifier against
--    `challenge`, and only then sets the session cookie.
--
-- The raw token never touches the database: only its SHA-256 is stored,
-- so a leaked DB can't be replayed. Rows are deleted on first use
-- whether or not the verifier matched, and expire after ~60 seconds.
CREATE TABLE mobile_handoffs (
    -- Lowercase hex SHA-256 of the token.
    token_hash  TEXT PRIMARY KEY NOT NULL,

    user_id     TEXT NOT NULL,

    -- Base64url SHA-256 of the app's verifier (PKCE S256).
    challenge   TEXT NOT NULL,

    expires_at  TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
