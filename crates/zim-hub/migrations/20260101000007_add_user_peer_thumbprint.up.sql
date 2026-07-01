-- RFC 7638 JWK thumbprint for each enrolled peer key.
-- SHA-256 of {"crv":"Ed25519","kty":"OKP","x":"<base64url>"}, base64url-encoded.
-- This is the `sub`/`kid` used by self-sovereign JWTs from browser peers.
-- NULL for rows enrolled before this migration; they use the pubkey-hex kid path.
ALTER TABLE user_peers ADD COLUMN peer_thumbprint TEXT;

CREATE INDEX user_peers_thumbprint_idx ON user_peers(peer_thumbprint)
    WHERE peer_thumbprint IS NOT NULL;
