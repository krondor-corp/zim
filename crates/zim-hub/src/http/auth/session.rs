//! Mint a session for an email the caller has already authenticated.
//! Every login provider ends here, so what a session *is* — user row,
//! claims, cookie — can't drift between them.
//!
//! No account linking: an account belongs to the provider that created
//! it. The same email arriving through another provider is refused, as
//! is a Google sign-in whose `sub` differs from the one recorded.

use axum::http::header::SET_COOKIE;
use axum::response::{IntoResponse, Redirect, Response};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, EncodingKey, Header};

use crate::config::AuthConfig;
use crate::database::models::User;
use crate::database::types::DbAuthProvider;
use crate::http::auth::Claims;
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
    /// The provider, stored as `users.auth_provider`.
    fn provider(&self) -> DbAuthProvider {
        match self {
            Authority::Google { .. } => DbAuthProvider::Google,
            #[cfg(feature = "dev-login")]
            Authority::DevTrust => DbAuthProvider::DevTrust,
        }
    }

    /// The provider's stable account id, stored as `users.auth_subject`.
    fn subject(&self) -> Option<&str> {
        match self {
            Authority::Google { sub } => Some(sub),
            #[cfg(feature = "dev-login")]
            Authority::DevTrust => None,
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
    #[error("{email} signs in with {provider}, not {attempted}; accounts aren't linked across sign-in methods")]
    WrongProvider {
        email: String,
        provider: DbAuthProvider,
        attempted: DbAuthProvider,
    },
    #[error("{email} belongs to a different {provider} account")]
    SubjectMismatch {
        email: String,
        provider: DbAuthProvider,
    },
    #[error("user provisioning failed: {0}")]
    User(#[from] sqlx::Error),
    #[error("session token encode failed: {0}")]
    Token(#[from] jsonwebtoken::errors::Error),
}

/// Find-or-create the user for `email`, sign a session JWT, and
/// answer with the cookie + a redirect to `next` (an already-sanitized
/// same-origin path; see [`super::return_to`]).
pub async fn mint_session(
    state: &AppState,
    email: &str,
    name: &str,
    authority: Authority,
    next: &str,
) -> Result<Response, SessionError> {
    let user = find_or_create_user(state, email, name, &authority).await?;

    let now = Utc::now();
    let exp = now + Duration::seconds(COOKIE_MAX_AGE_SECS);
    let claims = Claims {
        sub: match &authority {
            Authority::Google { sub } => sub.clone(),
            #[cfg(feature = "dev-login")]
            Authority::DevTrust => format!("dev:{email}"),
        },
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
    let cookie = build_cookie(
        &state.auth.session_cookie(),
        &token,
        secure,
        COOKIE_MAX_AGE_SECS,
    );

    tracing::info!(
        email = user.email(),
        role = %user.role().label(),
        authority = %authority.provider(),
        "user signed in"
    );

    Ok(([(SET_COOKIE, cookie)], Redirect::to(next)).into_response())
}

/// Look up the user by email (refusing a cross-provider or
/// cross-account sign-in) or insert a fresh row bound to this provider,
/// admin-ness per [`Authority::grants_admin`]. Existing rows keep their
/// role — subsequent changes happen through `/_admin`.
async fn find_or_create_user(
    state: &AppState,
    email: &str,
    name: &str,
    authority: &Authority,
) -> Result<User, SessionError> {
    if let Some(existing) = User::find_by_email(email, &state.db).await? {
        return match check_identity(
            existing.auth_provider(),
            existing.auth_subject(),
            authority,
            email,
        )? {
            Identity::Known => Ok(existing),
            Identity::RecordSubject(sub) => {
                Ok(existing.record_auth_subject(sub, &state.db).await?)
            }
        };
    }
    let admin = authority.grants_admin(&state.auth, email);
    Ok(User::create(
        email,
        name,
        admin,
        admin,
        authority.provider(),
        authority.subject(),
        &state.db,
    )
    .await?)
}

/// What a sign-in for an existing account needs.
#[derive(Debug, PartialEq)]
enum Identity<'a> {
    /// Same provider, same (or no) subject.
    Known,
    /// Same provider; the row predates `auth_subject`, so record it.
    RecordSubject(&'a str),
}

fn check_identity<'a>(
    stored_provider: DbAuthProvider,
    stored_subject: Option<&str>,
    authority: &'a Authority,
    email: &str,
) -> Result<Identity<'a>, SessionError> {
    if stored_provider != authority.provider() {
        return Err(SessionError::WrongProvider {
            email: email.to_string(),
            provider: stored_provider,
            attempted: authority.provider(),
        });
    }
    match (stored_subject, authority.subject()) {
        (Some(stored), Some(sub)) if stored != sub => Err(SessionError::SubjectMismatch {
            email: email.to_string(),
            provider: authority.provider(),
        }),
        (None, Some(sub)) => Ok(Identity::RecordSubject(sub)),
        _ => Ok(Identity::Known),
    }
}

fn build_cookie(name: &str, token: &str, secure: bool, max_age_secs: i64) -> String {
    let secure_attr = if secure { "; Secure" } else { "" };
    format!("{name}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}{secure_attr}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::auth_config;

    #[test]
    fn google_sign_in_promotes_only_allow_listed_emails() {
        let hub = auth_config(&["alice@example.com"]);
        let google = Authority::Google { sub: "123".into() };

        assert!(google.grants_admin(&hub, "alice@example.com"));
        assert!(!google.grants_admin(&hub, "bob@example.com"));
    }

    #[cfg(feature = "dev-login")]
    #[test]
    fn dev_trust_makes_everyone_admin_without_an_allow_list() {
        let hub = auth_config(&[]);
        assert!(Authority::DevTrust.grants_admin(&hub, "bob@example.com"));
    }

    // ── No account linking ────────────────────────────────────────

    #[test]
    fn alice_signs_back_in_with_her_google_account() {
        let google = Authority::Google {
            sub: "alice-sub".into(),
        };
        let check = check_identity(
            DbAuthProvider::Google,
            Some("alice-sub"),
            &google,
            "alice@example.com",
        );
        assert_eq!(check.unwrap(), Identity::Known);
    }

    #[test]
    fn a_pre_migration_google_row_records_its_sub_on_next_sign_in() {
        let google = Authority::Google {
            sub: "alice-sub".into(),
        };
        let check = check_identity(DbAuthProvider::Google, None, &google, "alice@example.com");
        assert_eq!(check.unwrap(), Identity::RecordSubject("alice-sub"));
    }

    #[test]
    fn a_different_google_account_with_alices_email_is_refused() {
        // e.g. the address was reassigned to someone else's Google account.
        let mallory = Authority::Google {
            sub: "mallory-sub".into(),
        };
        let err = check_identity(
            DbAuthProvider::Google,
            Some("alice-sub"),
            &mallory,
            "alice@example.com",
        );
        assert!(matches!(err, Err(SessionError::SubjectMismatch { .. })));
    }

    #[test]
    fn an_account_from_another_provider_is_never_linked() {
        let google = Authority::Google {
            sub: "alice-sub".into(),
        };
        let err = check_identity(DbAuthProvider::DevTrust, None, &google, "alice@example.com");
        assert!(matches!(
            err,
            Err(SessionError::WrongProvider {
                attempted: DbAuthProvider::Google,
                ..
            })
        ));
    }

    #[cfg(feature = "dev-login")]
    #[test]
    fn dev_trust_cannot_sign_into_a_google_account() {
        let err = check_identity(
            DbAuthProvider::Google,
            Some("alice-sub"),
            &Authority::DevTrust,
            "alice@example.com",
        );
        assert!(matches!(
            err,
            Err(SessionError::WrongProvider {
                attempted: DbAuthProvider::DevTrust,
                ..
            })
        ));
    }

    #[test]
    fn cookie_is_secure_only_over_https() {
        assert!(build_cookie("session", "t", true, 60).ends_with("; Secure"));
        assert!(!build_cookie("session", "t", false, 60).contains("Secure"));
    }
}
