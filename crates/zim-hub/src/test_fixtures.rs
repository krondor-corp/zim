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

/// A user to create: `user(&db, "alice").await` is an approved,
/// non-admin Google user `alice@example.com`; chain switches to change
/// that (`user(&db, "bob").admin().await`). Awaiting it inserts the row.
pub fn user<'a>(db: &'a Database, name: &'a str) -> UserFixture<'a> {
    UserFixture {
        db,
        name,
        admin: false,
    }
}

pub struct UserFixture<'a> {
    db: &'a Database,
    name: &'a str,
    admin: bool,
}

impl UserFixture<'_> {
    /// Make them a hub admin.
    pub fn admin(mut self) -> Self {
        self.admin = true;
        self
    }
}

impl<'a> std::future::IntoFuture for UserFixture<'a> {
    type Output = User;
    type IntoFuture = std::pin::Pin<Box<dyn std::future::Future<Output = User> + Send + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            let email = format!("{}@example.com", self.name);
            let sub = format!("{}-sub", self.name);
            User::create(
                &email,
                self.name,
                self.admin,
                true,
                "google",
                Some(&sub),
                self.db,
            )
            .await
            .expect("create fixture user")
        })
    }
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
