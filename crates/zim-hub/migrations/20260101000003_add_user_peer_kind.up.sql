-- Distinguish device classes registered to a user.
--
-- 'web'    — browser-resident ed25519 keypair, passphrase-wrapped
--            in escrow. Exactly one per user (enforced by the
--            partial unique index below). Mandatory for the web UI
--            gate; created via the browser onboarding flow.
-- 'daemon' — a `zim` daemon's identity key. Many per user.
--            Self-enrolled via the OAuth-from-daemon flow.
-- (future) 'claimed' — a pubkey the user knows about but doesn't
--            control. For declaring intent (e.g. share targets)
--            without possession proof. Not implemented yet.

ALTER TABLE user_peers
    ADD COLUMN kind TEXT NOT NULL DEFAULT 'daemon';

-- Partial unique index: at most one 'web' row per user. Daemons
-- can be many. Lets us key the onboarding gate on
-- `kind = 'web'` without scanning.
CREATE UNIQUE INDEX user_peers_one_web_per_user
    ON user_peers(user_id) WHERE kind = 'web';
