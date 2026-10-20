//! Mint a session for an email the caller has already authenticated.
//! Both login paths (Google OAuth, dev trust) end here, so what a
//! session *is* — user row, claims, cookie — can't drift between them.

use axum::http::header::SET_COOKIE;
use axum::response::{IntoResponse, Redirect, Response};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, EncodingKey, Header};

use crate::config::AuthConfig;
use crate::database::models::User;
use crate::http::auth::{Claims, SESSION_COOKIE};
use crate::state::AppState;

pub const COOKIE_MAX_AGE_SECS: i64 = 60 * 60 * 24 * 7;

/// Who authenticated the email.
pub enum Authority {
    /// Google asserted the email; `sub` is Google's stable account id.
    Google { sub: String },
    /// The operator typed it. Dev-trust users are created as admin +
    /// authorized: there is no one else to approve them.
    #[cfg(feature = "dev-login")]
    DevTrust,
}

impl Authority {
    fn label(&self) -> &'static str {
        match self {
            Authority::Google { .. } => "google",
            #[cfg(feature = "dev-login")]
            Authority::DevTrust => "dev-trust",
        }
    }

    fn subject(&self, email: &str) -> String {
        match self {
            Authority::Google { sub } => {
                let _ = email; // only the dev-trust arm derives sub from it
                sub.clone()
            }
            #[cfg(feature = "dev-login")]
            Authority::DevTrust => format!("dev:{email}"),
        }
    }

    /// Whether a brand-new user row gets admin + authorized. Google:
    /// the `admin_emails` allow-list bootstraps the first admin. Dev
    /// trust: everyone is admin — no allow-list needed in dev.
    fn grants_admin(&self, auth: &AuthConfig, email: &str) -> bool {
        match self {
            Authority::Google { .. } => auth.is_bootstrap_admin(email),
            #[cfg(feature = "dev-login")]
            Authority::DevTrust => true,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("user provisioning failed: {0}")]
    User(#[from] sqlx::Error),
    #[error("session token encode failed: {0}")]
    Token(#[from] jsonwebtoken::errors::Error),
}

/// Find-or-create the user for `email`, sign a session JWT, and
/// answer with the cookie + a redirect home.
pub async fn mint_session(
    state: &AppState,
    email: &str,
    name: &str,
    authority: Authority,
) -> Result<Response, SessionError> {
    let user = find_or_create_user(state, email, name, &authority).await?;

    let now = Utc::now();
    let exp = now + Duration::seconds(COOKIE_MAX_AGE_SECS);
    let claims = Claims {
        sub: authority.subject(email),
        email: user.email().to_string(),
        name: user.name().to_string(),
        iat: now.timestamp() as usize,
        exp: exp.timestamp() as usize,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.auth.service_secret.as_bytes()),
    )?;

    let secure = state.auth.host_name.starts_with("https://");
    let cookie = build_cookie(&token, secure, COOKIE_MAX_AGE_SECS);

    tracing::info!(
        email = user.email(),
        role = %user.role().label(),
        authority = authority.label(),
        "user signed in"
    );

    // Land authenticated users in the workspace, not on the public
    // marketing page.
    Ok(([(SET_COOKIE, cookie)], Redirect::to("/")).into_response())
}

/// Look up the user by email or insert a fresh row, admin-ness per
/// [`Authority::grants_admin`]. Existing rows keep their role —
/// subsequent changes happen through `/_admin`.
async fn find_or_create_user(
    state: &AppState,
    email: &str,
    name: &str,
    authority: &Authority,
) -> Result<User, sqlx::Error> {
    if let Some(existing) = User::find_by_email(email, &state.db).await? {
        return Ok(existing);
    }
    let admin = authority.grants_admin(&state.auth, email);
    User::create(email, name, admin, admin, &state.db).await
}

fn build_cookie(token: &str, secure: bool, max_age_secs: i64) -> String {
    let secure_attr = if secure { "; Secure" } else { "" };
    format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}{secure_attr}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AuthMode;

    fn hub_with_admins(admins: &[&str]) -> AuthConfig {
        AuthConfig {
            host_name: "http://127.0.0.1:17190".into(),
            admin_emails: admins.iter().map(|s| s.to_string()).collect(),
            mode: AuthMode::Google {
                client_id: "id".into(),
                client_secret: "secret".into(),
            },
            service_secret: "secret".into(),
        }
    }

    #[test]
    fn google_sign_in_promotes_only_allow_listed_emails() {
        let hub = hub_with_admins(&["alice@example.com"]);
        let google = Authority::Google { sub: "123".into() };

        assert!(google.grants_admin(&hub, "alice@example.com"));
        assert!(!google.grants_admin(&hub, "bob@example.com"));
    }

    #[test]
    fn google_session_subject_is_googles_account_id() {
        let google = Authority::Google { sub: "123".into() };
        assert_eq!(google.subject("alice@example.com"), "123");
    }

    #[cfg(feature = "dev-login")]
    #[test]
    fn dev_trust_makes_everyone_admin_without_an_allow_list() {
        let hub = hub_with_admins(&[]);
        assert!(Authority::DevTrust.grants_admin(&hub, "bob@example.com"));
        assert_eq!(
            Authority::DevTrust.subject("bob@example.com"),
            "dev:bob@example.com"
        );
    }

    #[test]
    fn cookie_is_secure_only_over_https() {
        assert!(build_cookie("t", true, 60).ends_with("; Secure"));
        assert!(!build_cookie("t", false, 60).contains("Secure"));
    }
}
