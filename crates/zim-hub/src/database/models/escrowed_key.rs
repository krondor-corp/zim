//! Passphrase-wrapped browser identity blob.
//!
//! The hub never sees the unwrapped key or the passphrase — it just
//! stores the ciphertext blob keyed by the DID verification-method
//! fragment. The browser fetches the blob, derives the wrap key from
//! the user's passphrase, and unwraps locally.
//!
//! Access control: every read/write/delete is gated on the
//! `did_fragment`'s `u:<user_uuid>` segment matching the requesting
//! user (or admin) — see [`crate::access::can_access_escrow_did`].
//! That keeps the row "public-by-knowledge-of-fragment" model
//! honest under multi-tenant.

use serde::Serialize;

use crate::database::Database;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct EscrowedKey {
    did_fragment: String,
    salt: Vec<u8>,
    kdf: String,
    wrapped_secret: Vec<u8>,
    created_at: String,
}

impl EscrowedKey {
    pub fn did_fragment(&self) -> &str {
        &self.did_fragment
    }
    pub fn salt(&self) -> &[u8] {
        &self.salt
    }
    pub fn kdf(&self) -> &str {
        &self.kdf
    }
    pub fn wrapped_secret(&self) -> &[u8] {
        &self.wrapped_secret
    }
    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    /// First-write-wins insert. Returns the freshly created row.
    /// The PK on `did_fragment` blocks duplicates; the caller maps
    /// that to a 409 Conflict.
    pub async fn create(
        did_fragment: &str,
        salt: &[u8],
        kdf: &str,
        wrapped_secret: &[u8],
        db: &Database,
    ) -> Result<Self, sqlx::Error> {
        sqlx::query_as::<_, EscrowedKey>(
            r#"
            INSERT INTO escrowed_keys (did_fragment, salt, kdf, wrapped_secret)
            VALUES (?, ?, ?, ?)
            RETURNING *
            "#,
        )
        .bind(did_fragment)
        .bind(salt)
        .bind(kdf)
        .bind(wrapped_secret)
        .fetch_one(&**db)
        .await
    }

    /// All fragments belonging to a user. The fragment convention
    /// is `did:web:<host>:u:<user_uuid>#<label>` — match on the
    /// `:u:<uuid>#` segment. Used by the browser's key-recovery
    /// path: a fresh browser knows the user (session cookie) but
    /// not which fragment its wrapped key was escrowed under.
    pub async fn list_for_user(
        user_id: uuid::Uuid,
        db: &Database,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, EscrowedKey>(
            "SELECT * FROM escrowed_keys WHERE did_fragment LIKE ? ORDER BY created_at DESC",
        )
        .bind(format!("%:u:{user_id}#%"))
        .fetch_all(&**db)
        .await
    }

    pub async fn find(did_fragment: &str, db: &Database) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, EscrowedKey>("SELECT * FROM escrowed_keys WHERE did_fragment = ?")
            .bind(did_fragment)
            .fetch_optional(&**db)
            .await
    }

    pub async fn delete(did_fragment: &str, db: &Database) -> Result<bool, sqlx::Error> {
        let res = sqlx::query("DELETE FROM escrowed_keys WHERE did_fragment = ?")
            .bind(did_fragment)
            .execute(&**db)
            .await?;
        Ok(res.rows_affected() > 0)
    }
}
