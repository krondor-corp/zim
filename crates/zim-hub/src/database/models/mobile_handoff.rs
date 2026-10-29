//! One-time handoff from a system web-auth session to the native app's
//! webview. See the migration for the lifecycle. Three facts keep it
//! safe to pass through a custom URL scheme that another app could
//! also claim:
//!
//! 1. The token is random, stored only as a SHA-256, and good for
//!    [`TTL_SECONDS`].
//! 2. [`MobileHandoff::consume`] deletes the row *before* looking at
//!    the verifier, so a token can be tried exactly once.
//! 3. Redeeming needs the verifier whose hash was registered at start
//!    (PKCE S256) — a token lifted off the callback URL is useless
//!    without it.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{Duration, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::database::types::DbUuid;
use crate::database::Database;

/// How long the app has to redeem a token after the handshake.
pub const TTL_SECONDS: i64 = 60;

/// Why a token was refused. All of them look the same to the caller.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// Unknown, already used, or past its TTL.
    NoSuchToken,
    /// The verifier doesn't hash to the challenge from sign-in start.
    WrongVerifier,
}

pub struct MobileHandoff;

impl MobileHandoff {
    /// Issue a token for `user_id`, redeemable by whoever holds the
    /// verifier behind `challenge`. Returns the raw token (never stored).
    pub async fn issue(
        user_id: uuid::Uuid,
        challenge: &str,
        db: &Database,
    ) -> Result<String, sqlx::Error> {
        Self::issue_for(user_id, challenge, Duration::seconds(TTL_SECONDS), db).await
    }

    pub(crate) async fn issue_for(
        user_id: uuid::Uuid,
        challenge: &str,
        ttl: Duration,
        db: &Database,
    ) -> Result<String, sqlx::Error> {
        // Sweep abandoned tokens so the table stays bounded.
        sqlx::query(
            "DELETE FROM mobile_handoffs \
             WHERE expires_at <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        )
        .execute(&**db)
        .await?;
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        let token = URL_SAFE_NO_PAD.encode(bytes);
        let expires_at = (Utc::now() + ttl).format("%Y-%m-%dT%H:%M:%SZ").to_string();
        sqlx::query(
            "INSERT INTO mobile_handoffs (token_hash, user_id, challenge, expires_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(hash_token(&token))
        .bind(DbUuid::from(user_id))
        .bind(challenge)
        .bind(expires_at)
        .execute(&**db)
        .await?;
        Ok(token)
    }

    /// Redeem `token` with `verifier`: the user to sign in. The row is
    /// gone afterwards, whatever the outcome.
    pub async fn consume(
        token: &str,
        verifier: &str,
        db: &Database,
    ) -> Result<Result<uuid::Uuid, Refused>, sqlx::Error> {
        let row = sqlx::query_as::<_, (DbUuid, String)>(
            r#"
            DELETE FROM mobile_handoffs
            WHERE token_hash = ?
              AND expires_at > strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            RETURNING user_id, challenge
            "#,
        )
        .bind(hash_token(token))
        .fetch_optional(&**db)
        .await?;
        Ok(match row {
            None => Err(Refused::NoSuchToken),
            Some((user_id, challenge)) if challenge_for(verifier) == challenge => {
                Ok(user_id.into())
            }
            Some(_) => Err(Refused::WrongVerifier),
        })
    }
}

/// The challenge the app registers for `verifier`: base64url(SHA-256).
pub fn challenge_for(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}
