-- identity_keys: zim-hub-side encrypted-blob vault, keyed by Google `sub`.
--
-- Storage model (per T-001 proposal):
--   * Hub stores ChaCha20-Poly1305-encrypted ed25519 secrets indexed by the
--     viewer's Google `sub` identifier.
--   * The KEK is derived browser-side via Argon2id from a viewer-chosen
--     password — the hub never sees the password and cannot decrypt the blob.
--   * `public_key` is the matching ed25519 public key (hex). Surfaced to
--     bucket owners so they can authorize the viewer out-of-band.

CREATE TABLE identity_keys (
    google_sub      TEXT    PRIMARY KEY,
    google_email    TEXT    NOT NULL,
    public_key      TEXT    NOT NULL,
    encrypted_blob  BLOB    NOT NULL,
    salt            BLOB    NOT NULL,
    kdf_params      TEXT    NOT NULL,
    created_at      INTEGER NOT NULL,
    last_used_at    INTEGER NOT NULL,
    UNIQUE (public_key)
);

CREATE INDEX idx_identity_keys_public_key ON identity_keys (public_key);
