//! Single-use possession-proof challenge for device enrollment.
//!
//! Flow:
//! 1. Client (browser JS or daemon CLI) calls
//!    `GET /api/v0/devices/enroll-challenge`. Hub allocates a fresh
//!    32-byte random challenge tied to the authenticated user with
//!    a 5-minute TTL.
//! 2. Client signs `challenge_bytes || pubkey_bytes` with the
//!    device's ed25519 private key.
//! 3. Client POSTs `(pubkey, signature, challenge, label, kind)` to
//!    `/api/v0/devices/self`. Hub verifies the challenge belongs to
//!    the requesting user, isn't expired, verifies the signature
//!    against `pubkey`, inserts the `user_peers` row, deletes the
//!    challenge.
//!
//! The hex of the challenge bytes IS the primary key — single-use
//! is enforced at the DB layer.

use chrono::{Duration, Utc};
use rand::RngCore;
use serde::Serialize;

use crate::database::types::DbUuid;
use crate::database::Database;

const CHALLENGE_BYTES: usize = 32;
const TTL_SECONDS: i64 = 5 * 60;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct EnrollChallenge {
    challenge: String,
    user_id: DbUuid,
    expires_at: String,
    created_at: String,
}

impl EnrollChallenge {
    pub fn challenge_hex(&self) -> &str {
        &self.challenge
    }
    pub fn expires_at(&self) -> &str {
        &self.expires_at
    }

    /// Decode the challenge from hex. `None` if the row is
    /// corrupted (length != 32 bytes); the verify step would fail
    /// anyway, but failing here lets callers distinguish bad-row
    /// from bad-signature.
    pub fn challenge_bytes(&self) -> Option<[u8; CHALLENGE_BYTES]> {
        let mut out = [0u8; CHALLENGE_BYTES];
        hex::decode_to_slice(&self.challenge, &mut out).ok()?;
        Some(out)
    }

    /// Allocate a fresh challenge for `user_id`. Returns the row;
    /// the hex string in `.challenge_hex()` is what the caller
    /// sends back over the wire.
    pub async fn create(user_id: uuid::Uuid, db: &Database) -> Result<Self, sqlx::Error> {
        let mut bytes = [0u8; CHALLENGE_BYTES];
        rand::thread_rng().fill_bytes(&mut bytes);
        let challenge = hex::encode(bytes);
        // ISO-8601 UTC. Same format as the `created_at` default so
        // string compares Just Work in the expiry check.
        let expires_at = (Utc::now() + Duration::seconds(TTL_SECONDS))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        sqlx::query_as::<_, EnrollChallenge>(
            r#"
            INSERT INTO enroll_challenges (challenge, user_id, expires_at)
            VALUES (?, ?, ?)
            RETURNING *
            "#,
        )
        .bind(&challenge)
        .bind(DbUuid::from(user_id))
        .bind(&expires_at)
        .fetch_one(&**db)
        .await
    }

    /// Look up a challenge by its hex form, scoped to the user that
    /// issued it. `Ok(None)` covers missing, expired, AND
    /// belongs-to-someone-else — callers map all three to the same
    /// "challenge not valid" client error so we don't leak which.
    pub async fn find_live_for_user(
        challenge: &str,
        user_id: uuid::Uuid,
        db: &Database,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, EnrollChallenge>(
            r#"
            SELECT * FROM enroll_challenges
            WHERE challenge = ? AND user_id = ?
              AND expires_at > strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            "#,
        )
        .bind(challenge)
        .bind(DbUuid::from(user_id))
        .fetch_optional(&**db)
        .await
    }

    /// Single-use: delete the row. Called by the enrollment handler
    /// only after the signature has verified — a half-finished
    /// enrollment leaves the challenge alive so the user can retry.
    pub async fn consume(challenge: &str, db: &Database) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM enroll_challenges WHERE challenge = ?")
            .bind(challenge)
            .execute(&**db)
            .await?;
        Ok(())
    }

    /// House-keeping: drop expired rows. The handler doesn't call
    /// this on the hot path; a periodic job (or a startup sweep)
    /// keeps the table bounded.
    pub async fn cleanup_expired(db: &Database) -> Result<u64, sqlx::Error> {
        let res = sqlx::query(
            "DELETE FROM enroll_challenges
             WHERE expires_at <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        )
        .execute(&**db)
        .await?;
        Ok(res.rows_affected())
    }
}
