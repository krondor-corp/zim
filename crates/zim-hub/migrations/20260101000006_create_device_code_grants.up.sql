-- Pairing codes for the device-code flow.
--
-- Lifecycle:
-- 1. `POST /api/v0/auth/device-code/start` (public) creates a row
--    with a fresh 9-char code AND the daemon's claimed pubkey,
--    no `user_id` yet, 10-min `expires_at`. The daemon commits to
--    the pubkey here so the user knows exactly what they're
--    authorizing before they click Approve.
-- 2. The user visits `/auth/device?code=…` in a browser. The page
--    redirects to OAuth if needed, then shows
--    "Pair daemon with pubkey <hex> as <label>?". One click.
-- 3. `POST /auth/device/approve` (RequireUser) sets `user_id` +
--    `approved_at` on the row.
-- 4. `POST /api/v0/auth/device-code/poll` (public) takes the code
--    and an ed25519 signature over `code_bytes || pubkey_bytes`.
--    Hub verifies the signature against the row's pubkey, inserts
--    `user_peers`, deletes the grant. Atomic enrollment.
--
-- From step 4 on, the daemon authenticates by signing short-lived
-- JWTs with the same identity key. No long-term server-side
-- session state needed.

CREATE TABLE device_code_grants (
    -- 9 chars, format `ABCD-EFGH` (uppercase letters + digits, no
    -- ambiguous characters). Human-typable; the daemon prints it.
    code            TEXT PRIMARY KEY NOT NULL,

    -- 64-char lowercase hex. The daemon commits to its pubkey at
    -- start-time so the approve page can render it for the user
    -- and the poll-time signature has a fixed target to verify
    -- against.
    pubkey          TEXT NOT NULL,

    -- Daemon-supplied label. Surfaced on the approve page so the
    -- user sees "Approve <label>?" rather than just a pubkey.
    label           TEXT NOT NULL DEFAULT '',

    -- NULL until the user clicks Approve.
    user_id         TEXT,
    approved_at     TEXT,

    expires_at      TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
