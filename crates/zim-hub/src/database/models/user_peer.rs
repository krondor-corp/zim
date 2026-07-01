use serde::Serialize;
use uuid::Uuid;
use zim_crypto::PublicKey;

use crate::database::types::DbUuid;
use crate::database::Database;

/// Device-class tag. Stored as TEXT in the `user_peers.kind` column.
///
/// - `Web` — browser-resident ed25519 keypair, passphrase-wrapped
///   in escrow. At most one per user (enforced by a partial unique
///   index). Drives the web-UI onboarding gate.
/// - `Daemon` — a `zim` daemon's identity key. Many per user,
///   enrolled via the OAuth-from-daemon flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PeerKind {
    Web,
    Daemon,
}

impl PeerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            PeerKind::Web => "web",
            PeerKind::Daemon => "daemon",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "web" => Some(PeerKind::Web),
            "daemon" => Some(PeerKind::Daemon),
            _ => None,
        }
    }
}

/// One peer (daemon or browser) registered to a user. The pubkey is
/// the primary key — a given pubkey can only belong to one user.
/// Vault access checks JOIN on this: a user can read vault V iff
/// one of their `UserPeer.peer_pubkey` entries matches a
/// shareholder on V's head manifest.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct UserPeer {
    peer_pubkey: String,
    user_id: DbUuid,
    /// Human label. `None` for a web key (the account's single master
    /// identity needs no name); daemons set one to tell devices apart.
    label: Option<String>,
    /// 'web' | 'daemon'. Stored as text — see [`PeerKind`].
    kind: String,
    created_at: String,
    /// RFC 7638 JWK thumbprint — `None` for rows enrolled before the
    /// thumbprint migration. These peers use the pubkey-hex JWT path.
    peer_thumbprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct UserPeerListItem {
    peer_pubkey: String,
    user_id: DbUuid,
    label: Option<String>,
    kind: String,
    created_at: String,
}

impl UserPeerListItem {
    pub fn peer_pubkey_hex(&self) -> &str {
        &self.peer_pubkey
    }
    pub fn peer_pubkey(&self) -> Option<PublicKey> {
        PublicKey::from_hex(&self.peer_pubkey).ok()
    }
    pub fn user_id(&self) -> Uuid {
        self.user_id.into()
    }
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
    pub fn kind(&self) -> &str {
        &self.kind
    }
    pub fn created_at(&self) -> &str {
        &self.created_at
    }
}

impl UserPeer {
    pub fn peer_pubkey_hex(&self) -> &str {
        &self.peer_pubkey
    }
    pub fn peer_pubkey(&self) -> Option<PublicKey> {
        PublicKey::from_hex(&self.peer_pubkey).ok()
    }
    pub fn user_id(&self) -> Uuid {
        self.user_id.into()
    }
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
    pub fn kind(&self) -> &str {
        &self.kind
    }
    pub fn created_at(&self) -> &str {
        &self.created_at
    }
    pub fn peer_thumbprint(&self) -> Option<&str> {
        self.peer_thumbprint.as_deref()
    }

    /// Register `pubkey` as belonging to `user_id` with the given
    /// device class. The PK constraint on `peer_pubkey` blocks
    /// cross-user duplicates; the partial unique index on
    /// `(user_id) WHERE kind = 'web'` blocks a user from registering
    /// a second web key. Both surface as sqlx `Database` errors the
    /// caller can map to 409.
    pub async fn create(
        user_id: Uuid,
        pubkey: &PublicKey,
        label: Option<&str>,
        kind: PeerKind,
        db: &Database,
    ) -> Result<Self, sqlx::Error> {
        let thumbprint = pubkey.jwk_thumbprint();
        sqlx::query_as::<_, UserPeer>(
            r#"
            INSERT INTO user_peers (peer_pubkey, user_id, label, kind, peer_thumbprint)
            VALUES (?, ?, ?, ?, ?)
            RETURNING *
            "#,
        )
        .bind(pubkey.to_hex())
        .bind(DbUuid::from(user_id))
        .bind(label)
        .bind(kind.as_str())
        .bind(thumbprint)
        .fetch_one(&**db)
        .await
    }

    /// Look up a peer by its JWK thumbprint. Used by the browser JWT
    /// bearer path (`kid` = thumbprint, not pubkey hex).
    pub async fn find_by_thumbprint(
        thumbprint: &str,
        db: &Database,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, UserPeer>("SELECT * FROM user_peers WHERE peer_thumbprint = ?")
            .bind(thumbprint)
            .fetch_optional(&**db)
            .await
    }

    pub async fn delete_for_user(
        user_id: Uuid,
        pubkey: &PublicKey,
        db: &Database,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query("DELETE FROM user_peers WHERE peer_pubkey = ? AND user_id = ?")
            .bind(pubkey.to_hex())
            .bind(DbUuid::from(user_id))
            .execute(&**db)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn list_for_user(
        user_id: Uuid,
        db: &Database,
    ) -> Result<Vec<UserPeerListItem>, sqlx::Error> {
        sqlx::query_as::<_, UserPeerListItem>(
            "SELECT * FROM user_peers WHERE user_id = ? ORDER BY created_at DESC",
        )
        .bind(DbUuid::from(user_id))
        .fetch_all(&**db)
        .await
    }

    /// Lookup by pubkey alone. Used by the JWT bearer path: the
    /// signed token names a pubkey, and we need its owning user.
    pub async fn find_by_pubkey(
        pubkey: &PublicKey,
        db: &Database,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, UserPeer>("SELECT * FROM user_peers WHERE peer_pubkey = ?")
            .bind(pubkey.to_hex())
            .fetch_optional(&**db)
            .await
    }

    /// Hot-path check used by the vault gate: does `user_id` own
    /// `pubkey`?
    pub async fn user_owns_pubkey(
        user_id: Uuid,
        pubkey: &PublicKey,
        db: &Database,
    ) -> Result<bool, sqlx::Error> {
        let row =
            sqlx::query("SELECT 1 FROM user_peers WHERE peer_pubkey = ? AND user_id = ? LIMIT 1")
                .bind(pubkey.to_hex())
                .bind(DbUuid::from(user_id))
                .fetch_optional(&**db)
                .await?;
        Ok(row.is_some())
    }

    /// Used by the onboarding gate: does this user have a web key?
    /// Daemon-only users aren't considered "onboarded" for the web
    /// UI — they have to mint a browser-resident key first to
    /// satisfy the controller role.
    pub async fn user_has_web_key(user_id: Uuid, db: &Database) -> Result<bool, sqlx::Error> {
        let row =
            sqlx::query("SELECT 1 FROM user_peers WHERE user_id = ? AND kind = 'web' LIMIT 1")
                .bind(DbUuid::from(user_id))
                .fetch_optional(&**db)
                .await?;
        Ok(row.is_some())
    }
}
