-- Make user_peers.label nullable.
--
-- A web key is the account's single master identity and needs no label;
-- only daemons benefit from one (to tell devices apart). SQLite can't drop
-- a NOT NULL constraint in place, so recreate the table. Existing empty
-- labels become NULL.

CREATE TABLE user_peers_new (
    peer_pubkey     TEXT PRIMARY KEY NOT NULL,
    user_id         TEXT NOT NULL,
    label           TEXT,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    kind            TEXT NOT NULL DEFAULT 'daemon',
    peer_thumbprint TEXT
);

INSERT INTO user_peers_new (peer_pubkey, user_id, label, created_at, kind, peer_thumbprint)
SELECT peer_pubkey, user_id, NULLIF(label, ''), created_at, kind, peer_thumbprint
FROM user_peers;

DROP TABLE user_peers;
ALTER TABLE user_peers_new RENAME TO user_peers;

CREATE INDEX user_peers_user_idx ON user_peers(user_id);
CREATE UNIQUE INDEX user_peers_one_web_per_user ON user_peers(user_id) WHERE kind = 'web';
CREATE INDEX user_peers_thumbprint_idx ON user_peers(peer_thumbprint)
    WHERE peer_thumbprint IS NOT NULL;
