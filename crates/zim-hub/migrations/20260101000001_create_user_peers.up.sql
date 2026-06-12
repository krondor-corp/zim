-- User → peer pubkey index. One row per registered peer (daemon).
-- The peer pubkey is the primary key so a given pubkey can only
-- belong to one user — no shared identities, no peer-stealing.
--
-- This table powers vault access control: user X can read vault V
-- iff there exists a row in `user_peers` for X whose pubkey matches
-- some shareholder on V's head manifest (or X is admin).
--
-- Peers are registered self-serve from the hub UI: the user pastes
-- their daemon's hex pubkey (output of `zim id`) and a label. There
-- is no possession proof on registration — claiming someone else's
-- pubkey only makes their vault ids visible *if* they ever mirror
-- to this hub, and the content stays encrypted with shares the
-- attacker can't recover. A future possession-check tightens this.

CREATE TABLE user_peers (
    peer_pubkey     TEXT PRIMARY KEY NOT NULL,
    user_id         TEXT NOT NULL,
    label           TEXT NOT NULL DEFAULT '',
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX user_peers_user_idx ON user_peers(user_id);
