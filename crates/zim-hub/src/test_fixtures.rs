//! Shared test fixtures: an in-memory hub database with named users,
//! and auth config. Tests read as stories — Alice owns things, Bob is
//! the admin — so the setup lives here instead of in every test.

use zim_crypto::{PrivateKey, PublicKey};

use crate::config::{AuthConfig, AuthProvider, GoogleOAuth};
use crate::database::models::{PeerKind, User, UserPeer};
use crate::Database;

/// The Google provider with placeholder credentials.
pub fn google() -> AuthProvider {
    AuthProvider::Google(GoogleOAuth {
        client_id: "client-id".into(),
        client_secret: "client-secret".into(),
    })
}

/// A dev hub's auth config: Google sign-in, `admin_emails` bootstrap list.
pub fn auth_config(admin_emails: &[&str]) -> AuthConfig {
    AuthConfig {
        host_name: "http://127.0.0.1:17190".into(),
        admin_emails: admin_emails.iter().map(|e| e.to_string()).collect(),
        providers: vec![google()],
        service_secret: "service-secret".into(),
    }
}

/// Alice: an approved, non-admin user.
pub async fn alice(db: &Database) -> User {
    User::create(
        "alice@example.com",
        "alice",
        false,
        true,
        "google",
        Some("alice-sub"),
        db,
    )
    .await
    .expect("create alice")
}

/// Bob: the hub's admin.
pub async fn admin_bob(db: &Database) -> User {
    User::create(
        "bob@example.com",
        "bob",
        true,
        true,
        "google",
        Some("bob-sub"),
        db,
    )
    .await
    .expect("create bob")
}

/// Enroll a fresh browser web key for `user`; returns its public key
/// (what a vault manifest lists as a shareholder).
pub async fn enroll_web_key(db: &Database, user: &User) -> PublicKey {
    let key = PrivateKey::generate().public();
    UserPeer::create(user.id(), &key, None, PeerKind::Web, db)
        .await
        .expect("enroll web key");
    key
}
