//! Auth configuration: the set of login providers this hub offers, plus
//! the session-cookie secret and admin bootstrap list they share.
//!
//! `ZIM_HUB_AUTH` is a comma-separated provider list; unset means
//! `google`. Providers:
//!
//! - `google` — Google OAuth asserts the email. Needs
//!   `GOOGLE_O_AUTH_CLIENT_ID` / `GOOGLE_O_AUTH_CLIENT_SECRET`.
//! - `dev-trust` — the operator types an email and gets an admin
//!   session. Exists only in `--features dev-login` builds and is
//!   refused unless `HOST_NAME` is loopback.
//!
//! Several can be on at once (`ZIM_HUB_AUTH=dev-trust,google`); the
//! sign-in page then offers a choice.

use std::env;
use std::net::SocketAddr;
use std::path::Path;

use super::{
    env_first, generate_secret, read_file_config, write_file_config, ConfigError, CONFIG_FILE,
};

/// Auth configuration. Always required — the hub never serves
/// unauthenticated traffic to its workspace UI / API.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Public URL the hub is reachable at — `HOST_NAME` env, or
    /// `ZIM_HUB_BASE_URL`, or `http://<listen_address>` as fallback.
    /// Used to build the OAuth callback URL (`<host_name>/auth/google/callback`).
    pub host_name: String,
    /// Emails auto-promoted to admin on first Google sign-in. Bootstrap
    /// hint only — once a row exists, role changes happen via `/_admin`.
    /// Dev-trust sign-ins are always admin; there the first entry only
    /// pre-fills the login form. `ZIM_HUB_ADMIN_EMAILS=a@x,b@y`.
    pub admin_emails: Vec<String>,
    /// Enabled login providers, in `ZIM_HUB_AUTH` order (the order the
    /// sign-in page lists them). Never empty.
    pub providers: Vec<AuthProvider>,
    /// HS256 signing key for the session cookie. `SERVICE_SECRET`
    /// (generic convention) or `ZIM_HUB_SESSION_SECRET`. Auto-
    /// generated + persisted to `hub-config.toml` if missing.
    pub service_secret: String,
}

/// A way to assert a signing-in user's email.
#[derive(Debug, Clone)]
pub enum AuthProvider {
    /// Google OAuth asserts the email.
    Google(GoogleOAuth),
    /// The operator asserts the email. Compiled in only with the
    /// `dev-login` cargo feature; refused at runtime off loopback.
    #[cfg(feature = "dev-login")]
    DevTrust,
}

#[derive(Debug, Clone)]
pub struct GoogleOAuth {
    /// `GOOGLE_O_AUTH_CLIENT_ID` env (generic convention) or
    /// `ZIM_HUB_GOOGLE_CLIENT_ID` (zim legacy).
    pub client_id: String,
    /// Same resolution for the secret.
    pub client_secret: String,
}

impl AuthProvider {
    /// The provider's `ZIM_HUB_AUTH` token.
    pub fn name(&self) -> &'static str {
        match self {
            AuthProvider::Google(_) => "google",
            #[cfg(feature = "dev-login")]
            AuthProvider::DevTrust => "dev-trust",
        }
    }

    fn as_google(&self) -> Option<&GoogleOAuth> {
        match self {
            AuthProvider::Google(g) => Some(g),
            #[cfg(feature = "dev-login")]
            AuthProvider::DevTrust => None,
        }
    }

    /// Providers that trust the operator may only answer on loopback.
    fn is_loopback_only(&self) -> bool {
        match self {
            AuthProvider::Google(_) => false,
            #[cfg(feature = "dev-login")]
            AuthProvider::DevTrust => true,
        }
    }

    /// Logged at startup when this provider is on.
    pub fn startup_warning(&self) -> Option<&'static str> {
        match self {
            AuthProvider::Google(_) => None,
            #[cfg(feature = "dev-login")]
            AuthProvider::DevTrust => {
                Some("dev-trust login is on: ANY email signs in as admin. Dev only.")
            }
        }
    }
}

impl AuthConfig {
    pub(super) fn resolve(
        data_dir: &Path,
        listen_address: SocketAddr,
    ) -> Result<Self, ConfigError> {
        let config_path = data_dir.join(CONFIG_FILE);
        let mut file = read_file_config(&config_path)?;

        // `HOST_NAME` matches generic. `ZIM_HUB_BASE_URL` was the
        // earlier zim-hub name; accept both for one release.
        let host_name = env_first(&["HOST_NAME", "ZIM_HUB_BASE_URL"])
            .or_else(|| file.host_name.clone())
            .unwrap_or_else(|| format!("http://{listen_address}"));

        let admin_emails: Vec<String> = match env::var("ZIM_HUB_ADMIN_EMAILS") {
            Ok(s) => s
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            Err(_) => file.admin_emails.clone().unwrap_or_default(),
        };

        let google_o_auth_client_id =
            env_first(&["GOOGLE_O_AUTH_CLIENT_ID", "ZIM_HUB_GOOGLE_CLIENT_ID"])
                .or_else(|| file.google_o_auth_client_id.clone());
        let google_o_auth_client_secret = env_first(&[
            "GOOGLE_O_AUTH_CLIENT_SECRET",
            "ZIM_HUB_GOOGLE_CLIENT_SECRET",
        ])
        .or_else(|| file.google_o_auth_client_secret.clone());

        let providers = resolve_providers(
            env_first(&["ZIM_HUB_AUTH"]).as_deref(),
            &host_name,
            google_o_auth_client_id,
            google_o_auth_client_secret,
        )?;

        let service_secret = match env_first(&["SERVICE_SECRET", "ZIM_HUB_SESSION_SECRET"])
            .or_else(|| file.service_secret.clone())
        {
            Some(s) if !s.is_empty() => s,
            _ => {
                let generated = generate_secret();
                tracing::info!(
                    "no SERVICE_SECRET configured — generating one and persisting \
                     to {config_path}",
                    config_path = config_path.display()
                );
                file.service_secret = Some(generated.clone());
                write_file_config(&config_path, &file)?;
                generated
            }
        };

        Ok(Self {
            host_name,
            admin_emails,
            providers,
            service_secret,
        })
    }

    /// `{host_name}/auth/google/callback` — the redirect URI Google
    /// posts back to and that must match the OAuth client config.
    pub fn callback_url(&self) -> String {
        format!(
            "{}/auth/google/callback",
            self.host_name.trim_end_matches('/')
        )
    }

    /// The Google OAuth client, when Google sign-in is enabled.
    pub fn google(&self) -> Option<&GoogleOAuth> {
        self.providers.iter().find_map(AuthProvider::as_google)
    }

    /// True when `email` should bootstrap as admin on first Google
    /// sign-in. Used by session minting; subsequent admin appointments
    /// happen via `/_admin`.
    pub fn is_bootstrap_admin(&self, email: &str) -> bool {
        self.admin_emails.iter().any(|e| e == email)
    }
}

/// Parse the `ZIM_HUB_AUTH` provider list (unset → `google`). Fails
/// closed: an unknown or not-compiled-in provider, a provider missing
/// its credentials, or a loopback-only provider on a public host all
/// refuse to start rather than quietly dropping the provider.
fn resolve_providers(
    spec: Option<&str>,
    host_name: &str,
    google_client_id: Option<String>,
    google_client_secret: Option<String>,
) -> Result<Vec<AuthProvider>, ConfigError> {
    let mut providers: Vec<AuthProvider> = Vec::new();
    for name in spec.unwrap_or("google").split(',').map(str::trim) {
        if name.is_empty() || providers.iter().any(|p| p.name() == name) {
            continue;
        }
        let provider = match name {
            "google" => {
                let (Some(client_id), Some(client_secret)) =
                    (google_client_id.clone(), google_client_secret.clone())
                else {
                    return Err(ConfigError::AuthMissing);
                };
                AuthProvider::Google(GoogleOAuth {
                    client_id,
                    client_secret,
                })
            }
            #[cfg(feature = "dev-login")]
            "dev-trust" => AuthProvider::DevTrust,
            other => {
                return Err(ConfigError::UnknownAuthProvider {
                    name: other.to_string(),
                })
            }
        };
        if provider.is_loopback_only() && !is_loopback(host_name) {
            return Err(ConfigError::LoopbackOnlyProvider {
                provider: provider.name(),
                host_name: host_name.to_string(),
            });
        }
        providers.push(provider);
    }
    if providers.is_empty() {
        return Err(ConfigError::NoAuthProviders);
    }
    Ok(providers)
}

/// True when `host_name` is a URL on this machine.
fn is_loopback(host_name: &str) -> bool {
    url::Url::parse(host_name)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| matches!(h, "127.0.0.1" | "localhost" | "[::1]"))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PUBLIC: &str = "https://hub.zim.krondor.org";
    const LOOPBACK: &str = "http://127.0.0.1:17190";

    fn creds() -> (Option<String>, Option<String>) {
        (Some("client-id".into()), Some("client-secret".into()))
    }

    fn names(providers: &[AuthProvider]) -> Vec<&'static str> {
        providers.iter().map(AuthProvider::name).collect()
    }

    // ── Google: the default, in every build ────────────────────────

    #[test]
    fn production_hub_with_google_creds_offers_google() {
        let (id, secret) = creds();
        let providers = resolve_providers(None, PUBLIC, id, secret).unwrap();
        assert_eq!(names(&providers), ["google"]);
    }

    #[test]
    fn production_hub_without_google_creds_is_refused() {
        let err = resolve_providers(None, PUBLIC, None, None).unwrap_err();
        assert!(matches!(err, ConfigError::AuthMissing));
    }

    #[test]
    fn operator_listing_an_unknown_provider_is_refused() {
        let (id, secret) = creds();
        let err = resolve_providers(Some("google,github"), PUBLIC, id, secret).unwrap_err();
        assert!(matches!(err, ConfigError::UnknownAuthProvider { name } if name == "github"));
    }

    #[test]
    fn operator_listing_no_providers_is_refused() {
        let err = resolve_providers(Some(" , "), PUBLIC, None, None).unwrap_err();
        assert!(matches!(err, ConfigError::NoAuthProviders));
    }

    #[test]
    fn repeated_providers_collapse_to_one() {
        let (id, secret) = creds();
        let providers = resolve_providers(Some("google, google"), PUBLIC, id, secret).unwrap();
        assert_eq!(names(&providers), ["google"]);
    }

    // ── Default (release) build: dev-trust does not exist ──────────

    #[cfg(not(feature = "dev-login"))]
    #[test]
    fn release_build_refuses_dev_trust_even_on_loopback() {
        let err = resolve_providers(Some("dev-trust"), LOOPBACK, None, None).unwrap_err();
        assert!(matches!(err, ConfigError::UnknownAuthProvider { name } if name == "dev-trust"));
    }

    #[cfg(not(feature = "dev-login"))]
    #[test]
    fn release_build_has_only_the_google_provider() {
        // Exhaustive with a single arm: adding an un-gated provider breaks
        // this match and forces a look at what a release build can do.
        let (id, secret) = creds();
        for provider in resolve_providers(None, LOOPBACK, id, secret).unwrap() {
            match provider {
                AuthProvider::Google(_) => {}
            }
        }
    }

    // ── dev-login build: trust mode, loopback only ─────────────────

    #[cfg(feature = "dev-login")]
    #[test]
    fn dev_on_loopback_signs_in_without_google_creds() {
        for host in [LOOPBACK, "http://localhost:17190", "http://[::1]:17190"] {
            let providers = resolve_providers(Some("dev-trust"), host, None, None).unwrap();
            assert_eq!(names(&providers), ["dev-trust"], "{host}");
        }
    }

    #[cfg(feature = "dev-login")]
    #[test]
    fn dev_trust_on_a_public_host_refuses_to_start() {
        let (id, secret) = creds();
        let err = resolve_providers(Some("google,dev-trust"), PUBLIC, id, secret).unwrap_err();
        assert!(matches!(
            err,
            ConfigError::LoopbackOnlyProvider {
                provider: "dev-trust",
                ..
            }
        ));
    }

    #[cfg(feature = "dev-login")]
    #[test]
    fn dev_hub_can_offer_dev_trust_and_google_side_by_side() {
        let (id, secret) = creds();
        let providers = resolve_providers(Some("dev-trust,google"), LOOPBACK, id, secret).unwrap();
        assert_eq!(names(&providers), ["dev-trust", "google"]);
    }

    #[cfg(feature = "dev-login")]
    #[test]
    fn asking_for_google_alongside_dev_trust_still_needs_creds() {
        let err = resolve_providers(Some("dev-trust,google"), LOOPBACK, None, None).unwrap_err();
        assert!(matches!(err, ConfigError::AuthMissing));
    }
}
