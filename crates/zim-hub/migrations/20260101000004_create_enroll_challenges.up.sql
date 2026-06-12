-- Single-use possession-proof challenges for device enrollment.
--
-- Issued by `GET /api/v0/devices/enroll-challenge`. The caller signs
-- `challenge_bytes || pubkey_bytes` with the device's private key
-- and POSTs the result to `/api/v0/devices/self`. Hub verifies the
-- signature, inserts a `user_peers` row, deletes the challenge.
--
-- Bound to the issuing user via `user_id` so a leaked challenge can
-- only be used by that user's signed-in session — defends against
-- "attacker sends a victim's daemon a challenge issued to the
-- attacker" trick.
--
-- 5-minute TTL keeps the row count bounded under normal use and
-- closes the replay window if a signed payload leaks. A periodic
-- cleanup query removes expired rows; sqlite has no built-in TTL.

CREATE TABLE enroll_challenges (
    -- 32 random bytes as lowercase hex (64 chars). PK so a single
    -- challenge can only be used once — the consume step DELETEs.
    challenge   TEXT PRIMARY KEY NOT NULL,
    user_id     TEXT NOT NULL,
    -- ISO-8601 UTC string. Sortable lexically; sqlite compares as
    -- TEXT so `expires_at > strftime('%Y-%m-%dT%H:%M:%SZ', 'now')`
    -- works without a function call per row.
    expires_at  TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX enroll_challenges_user_idx ON enroll_challenges(user_id);
