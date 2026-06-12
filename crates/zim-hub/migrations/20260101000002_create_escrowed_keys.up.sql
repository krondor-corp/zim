-- Passphrase-wrapped browser identities. The row is "public-by-
-- knowledge-of-fragment" in terms of the ciphertext — anyone who
-- knows the DID can fetch the blob, and the passphrase is the
-- access control on the wrapped key.
--
-- Multi-tenant gate: the hub authorizes reads/writes/deletes by the
-- `did:web:<host>:u:<user_uuid>` segment in the fragment, so even
-- enumerating fragments requires being signed in as the owning
-- user. See `crate::access::can_access_escrow_did`.

CREATE TABLE escrowed_keys (
    -- Fully-qualified DID URL with the verification-method fragment:
    -- e.g. "did:web:hub.example.com:u:<user_uuid>#browser-laptop".
    did_fragment    TEXT PRIMARY KEY NOT NULL,

    -- Random salt fed into the KDF alongside the user's passphrase
    -- to derive the wrap key.
    salt            BLOB NOT NULL,

    -- KDF identifier + parameters as a single canonical string:
    --   "pbkdf2-sha256/100000"
    --   "argon2id/v=19,m=65536,t=3,p=4"
    -- Stored verbatim so a future row can use a stronger KDF
    -- without migrating older rows.
    kdf             TEXT NOT NULL,

    -- ChaCha20-Poly1305(kdf(passphrase, salt), ed25519_sk).
    -- AEAD ensures wrong-passphrase attempts fail loudly instead of
    -- yielding garbage.
    wrapped_secret  BLOB NOT NULL,

    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Cheap "list every fragment under a given user DID" lookup.
CREATE INDEX escrowed_keys_prefix_idx ON escrowed_keys(did_fragment);
