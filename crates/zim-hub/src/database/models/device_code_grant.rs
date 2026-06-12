//! Device-code pairing grant. See the migration for the full
//! lifecycle. Two facts shape the model:
//!
//! 1. The daemon commits to its pubkey at *start* time. The approve
//!    page renders it; the poll-time signature has a fixed target.
//!    A grant is a triple `(code, pubkey, label)` plus a pending
//!    approval slot.
//! 2. Enrollment is atomic with poll-when-approved. There's no
//!    standalone `session_token` field — daemons authenticate
//!    going forward by signing JWTs with the same key they signed
//!    the poll payload with.

use chrono::{Duration, Utc};
use rand::Rng;
use serde::Serialize;

use crate::database::types::DbUuid;
use crate::database::Database;

const TTL_SECONDS: i64 = 10 * 60;
/// Alphabet for the human-typable code. Ambiguous characters
/// removed: no `0`/`O`/`I`/`1`/`l`/`B`/`8` so a user reading the
/// daemon's terminal and typing into a phone doesn't pick wrong.
const CODE_ALPHABET: &[u8] = b"ACDEFGHJKLMNPQRSTUVWXYZ23456789";

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DeviceCodeGrant {
    code: String,
    pubkey: String,
    label: String,
    user_id: Option<DbUuid>,
    approved_at: Option<String>,
    expires_at: String,
    created_at: String,
}

impl DeviceCodeGrant {
    pub fn code(&self) -> &str {
        &self.code
    }
    pub fn pubkey_hex(&self) -> &str {
        &self.pubkey
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn user_id(&self) -> Option<uuid::Uuid> {
        self.user_id.map(|u| u.into())
    }
    pub fn approved_at(&self) -> Option<&str> {
        self.approved_at.as_deref()
    }
    pub fn expires_at(&self) -> &str {
        &self.expires_at
    }
    pub fn is_approved(&self) -> bool {
        self.user_id.is_some()
    }
    pub fn is_expired(&self) -> bool {
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.expires_at <= now
    }

    /// Generate a fresh code (`ABCD-EFGH`). Inserts the row with
    /// the daemon's claimed pubkey and label; no user_id yet. The
    /// caller returns code+expires_at to the daemon.
    pub async fn create(pubkey_hex: &str, label: &str, db: &Database) -> Result<Self, sqlx::Error> {
        // Loop on collision. With ~30^8 codespace this is
        // effectively never; the loop is a sanity net.
        for _ in 0..5 {
            let code = mint_code();
            let expires_at = (Utc::now() + Duration::seconds(TTL_SECONDS))
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string();
            let res = sqlx::query_as::<_, DeviceCodeGrant>(
                r#"
                INSERT INTO device_code_grants (code, pubkey, label, expires_at)
                VALUES (?, ?, ?, ?)
                RETURNING *
                "#,
            )
            .bind(&code)
            .bind(pubkey_hex)
            .bind(label)
            .bind(&expires_at)
            .fetch_one(&**db)
            .await;
            match res {
                Ok(row) => return Ok(row),
                Err(sqlx::Error::Database(e)) if e.message().contains("UNIQUE") => continue,
                Err(e) => return Err(e),
            }
        }
        Err(sqlx::Error::Protocol(
            "could not allocate a unique device code after retries".into(),
        ))
    }

    pub async fn find(code: &str, db: &Database) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, DeviceCodeGrant>("SELECT * FROM device_code_grants WHERE code = ?")
            .bind(code)
            .fetch_optional(&**db)
            .await
    }

    /// Stamp the grant as approved by `user_id`. Idempotent against
    /// double-approval by requiring the row to still be pending.
    pub async fn approve(
        code: &str,
        user_id: uuid::Uuid,
        db: &Database,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            r#"
            UPDATE device_code_grants
            SET user_id = ?,
                approved_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            WHERE code = ?
              AND user_id IS NULL
              AND expires_at > strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            "#,
        )
        .bind(DbUuid::from(user_id))
        .bind(code)
        .execute(&**db)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Consume on successful enrollment poll: delete the grant.
    pub async fn consume(code: &str, db: &Database) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM device_code_grants WHERE code = ?")
            .bind(code)
            .execute(&**db)
            .await?;
        Ok(())
    }

    /// House-keeping. Periodic job calls this so the table stays
    /// bounded; the request path doesn't need to sweep on every
    /// request.
    pub async fn cleanup_expired(db: &Database) -> Result<u64, sqlx::Error> {
        let res = sqlx::query(
            r#"
            DELETE FROM device_code_grants
            WHERE expires_at <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            "#,
        )
        .execute(&**db)
        .await?;
        Ok(res.rows_affected())
    }
}

fn mint_code() -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 8];
    for b in bytes.iter_mut() {
        *b = CODE_ALPHABET[rng.gen_range(0..CODE_ALPHABET.len())];
    }
    let left = std::str::from_utf8(&bytes[0..4]).unwrap();
    let right = std::str::from_utf8(&bytes[4..8]).unwrap();
    format!("{left}-{right}")
}
