//! Hub configuration.
//!
//! Resolution order for every field:
//!
//! 1. environment variable,
//! 2. `$ZIM_HUB_HOME/hub-config.toml` (gitignored; persisted across
//!    restarts),
//! 3. baked-in default (where one exists).
//!
//! Env var naming matches `krondor-corp/generic`'s rust app so a
//! single `.envrc` / `export` block powers both:
//!
//! | env var                          | role                       |
//! |----------------------------------|----------------------------|
//! | `GOOGLE_O_AUTH_CLIENT_ID`        | OAuth client id            |
//! | `GOOGLE_O_AUTH_CLIENT_SECRET`    | OAuth client secret        |
//! | `SERVICE_SECRET`                 | HS256 session signing key  |
//! | `HOST_NAME`                      | public base URL            |
//!
//! `ZIM_HUB_AUTH` picks the login mode. Unset → Google OAuth (the only
//! mode in a default build). `dev-trust` → the operator types an email
//! and gets an admin session, no Google involved; honoured only in
//! `--features dev-login` builds and only when `HOST_NAME` is loopback.
//!
//! Zim-specific knobs keep the `ZIM_HUB_` prefix so multi-process
//! deployments don't collide on listen address / data dir:
//! `ZIM_HUB_LISTEN`, `ZIM_HUB_HOME`, `ZIM_HUB_LOG`,
//! `ZIM_HUB_ADMIN_EMAILS`, and the blob-store backend:
//! `ZIM_HUB_S3_ENDPOINT` / `_ACCESS_KEY` / `_SECRET_KEY` /
//! `_BUCKET` / `_REGION` (all-or-nothing except region; unset →
//! local filesystem store). (Legacy `ZIM_HUB_GOOGLE_CLIENT_ID`,
//! `ZIM_HUB_GOOGLE_CLIENT_SECRET`, `ZIM_HUB_SESSION_SECRET`,
//! `ZIM_HUB_BASE_URL` are still accepted as fallbacks; the
//! generic-style names win when both are set.)
//!
//! Secrets follow the same resolution order but: never commit them.
//! The TOML file is meant to live in the operator's home dir, not
//! the repo. The `service_secret` is auto-generated and persisted
//! to the TOML on first start when absent — losing it invalidates
//! every outstanding cookie, which is fine.
//!
//! Multi-user model: the hub doesn't have an "owner" — it has a
//! `User` table (one row per Google login) with `is_admin` and
//! `is_authorized` flags. `admin_emails` here is just a bootstrap
//! hint: any email in the list is automatically `is_admin=true` on
//! first sign-in. Subsequent admin appointment happens via the
//! `/_admin` panel.

use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use rand::Rng;
use serde::{Deserialize, Serialize};

// Dev port convention: all zim dev services live in the 17xxx band;
// the hub owns 1719x (see bin/dev_/nodes.toml for the band map).
const DEFAULT_LISTEN: &str = "127.0.0.1:17190";
const DEFAULT_HOME: &str = "./data/zim-hub";
const CONFIG_FILE: &str = "hub-config.toml";

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_address: SocketAddr,
    /// Hub's data directory. Same on-disk layout as a regular `zim`
    /// daemon's `$ZIM_HOME` — identity key, vault log, blob store.
    pub data_dir: PathBuf,
    pub log_level: tracing::Level,
    /// Public hostname this hub answers as. Drives the `did:web` URL
    /// (`did:web:<host>`) and the URL the DID document declares as its
    /// `id`. Defaults to the listen address with port percent-encoded.
    ///
    /// Examples:
    /// - prod: `ZIM_HUB_HOST=hub.example.com` → `did:web:hub.example.com`
    /// - dev:  unset, listening on `127.0.0.1:17190` → `did:web:127.0.0.1%3A17190`
    pub host: String,
    pub auth: AuthConfig,
    /// S3-compatible blob-store backend. `None` → local filesystem
    /// store under `data_dir/blobs/`. Dev: `make hub` points this at
    /// the minio container from `bin/minio`.
    pub s3: Option<S3Config>,
}

/// S3-compatible object-store coordinates for the hub's blob
/// backend. Resolved from `ZIM_HUB_S3_*` env vars; the SQLite blob
/// index lives at `data_dir/blob-index.sqlite` regardless.
#[derive(Debug, Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    pub bucket: String,
    pub region: Option<String>,
}

impl S3Config {
    /// All-or-nothing from env: if `ZIM_HUB_S3_ENDPOINT` is set, the
    /// key/secret/bucket vars are required; if it's unset, the rest
    /// are ignored and the hub falls back to the filesystem store.
    fn from_env() -> Result<Option<Self>, ConfigError> {
        let Ok(endpoint) = env::var("ZIM_HUB_S3_ENDPOINT") else {
            return Ok(None);
        };
        let need = |var: &str| {
            env::var(var).map_err(|_| ConfigError::S3Incomplete {
                missing: var.to_string(),
            })
        };
        Ok(Some(Self {
            endpoint,
            access_key: need("ZIM_HUB_S3_ACCESS_KEY")?,
            secret_key: need("ZIM_HUB_S3_SECRET_KEY")?,
            bucket: need("ZIM_HUB_S3_BUCKET")?,
            region: env::var("ZIM_HUB_S3_REGION").ok(),
        }))
    }
}

/// Who asserts a signing-in user's email.
#[derive(Debug, Clone)]
pub enum AuthMode {
    /// Production: Google OAuth asserts the email.
    Google {
        /// `GOOGLE_O_AUTH_CLIENT_ID` env (generic convention) or
        /// `ZIM_HUB_GOOGLE_CLIENT_ID` (zim legacy).
        client_id: String,
        /// Same resolution for the secret.
        client_secret: String,
    },
    /// Dev only: the operator asserts the email. Compiled in only with
    /// the `dev-login` cargo feature; refused at runtime off loopback.
    #[cfg(feature = "dev-login")]
    DevTrust,
}

/// Auth configuration. Always required — the hub never serves
/// unauthenticated traffic to its workspace UI / API.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Public URL the hub is reachable at — `HOST_NAME` env, or
    /// `ZIM_HUB_BASE_URL`, or `http://<listen_address>` as fallback.
    /// Used to build the OAuth callback URL (`<host_name>/auth/google/callback`).
    pub host_name: String,
    /// Emails auto-promoted to admin on first sign-in in Google mode.
    /// Bootstrap hint only — once a row exists, role changes happen via
    /// `/_admin`. Irrelevant in dev-trust mode (everyone is admin), where
    /// the first entry only pre-fills the login form.
    /// Comma-separated in `ZIM_HUB_ADMIN_EMAILS=a@x,b@y`.
    pub admin_emails: Vec<String>,
    /// Who asserts the email at sign-in. `ZIM_HUB_AUTH` env.
    pub mode: AuthMode,
    /// HS256 signing key for the session cookie. `SERVICE_SECRET`
    /// (generic convention) or `ZIM_HUB_SESSION_SECRET`. Auto-
    /// generated + persisted to `hub-config.toml` if missing.
    pub service_secret: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid ZIM_HUB_LISTEN: {0}")]
    InvalidAddress(#[from] std::net::AddrParseError),
    #[error("io reading hub-config.toml: {0}")]
    Io(#[from] std::io::Error),
    #[error("hub-config.toml parse: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("hub-config.toml serialize: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error(
        "ZIM_HUB_S3_ENDPOINT is set but {missing} is not — the S3 blob store \
         needs endpoint, access key, secret key, and bucket"
    )]
    S3Incomplete { missing: String },
    #[error(
        "auth misconfigured: GOOGLE_O_AUTH_CLIENT_ID and GOOGLE_O_AUTH_CLIENT_SECRET \
         are required (or the ZIM_HUB_GOOGLE_CLIENT_ID / ZIM_HUB_GOOGLE_CLIENT_SECRET \
         legacy names). Get them from https://console.cloud.google.com/apis/credentials \
         (OAuth 2.0 Client IDs → Web application). Set the authorized redirect URI to \
         `<host_name>/auth/google/callback`."
    )]
    AuthMissing,
    #[error("ZIM_HUB_AUTH=dev-trust is only honoured when HOST_NAME is loopback; got {host_name}")]
    DevTrustOffLoopback { host_name: String },
}

/// `hub-config.toml` on-disk shape. Field names match the
/// generic-style env vars (lower_snake_case): `host_name`,
/// `google_o_auth_client_id`, etc. Legacy `ZIM_HUB_*`-derived
/// fields are also accepted as aliases for backward compat.
#[derive(Debug, Default, Serialize, Deserialize)]
struct FileConfig {
    #[serde(default)]
    admin_emails: Option<Vec<String>>,
    #[serde(default, alias = "base_url")]
    host_name: Option<String>,
    #[serde(default, alias = "google_client_id")]
    google_o_auth_client_id: Option<String>,
    #[serde(default, alias = "google_client_secret")]
    google_o_auth_client_secret: Option<String>,
    #[serde(default, alias = "session_secret")]
    service_secret: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let listen_address: SocketAddr = env::var("ZIM_HUB_LISTEN")
            .unwrap_or_else(|_| DEFAULT_LISTEN.to_string())
            .parse()?;

        let data_dir = env::var("ZIM_HUB_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_HOME));

        let log_level = env::var("ZIM_HUB_LOG")
            .unwrap_or_else(|_| "info".to_string())
            .parse()
            .unwrap_or(tracing::Level::INFO);

        let host = env::var("ZIM_HUB_HOST")
            .unwrap_or_else(|_| listen_address.to_string().replace(':', "%3A"));

        let auth = AuthConfig::resolve(&data_dir, listen_address)?;
        let s3 = S3Config::from_env()?;

        Ok(Self {
            listen_address,
            data_dir,
            log_level,
            host,
            auth,
            s3,
        })
    }

    /// `did:web:<host>` — the hub's canonical identity URL.
    pub fn did(&self) -> String {
        format!("did:web:{}", self.host)
    }
}

impl AuthConfig {
    fn resolve(data_dir: &Path, listen_address: SocketAddr) -> Result<Self, ConfigError> {
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

        let mode = resolve_mode(
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
            mode,
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

    /// Short name for the startup banner.
    pub fn mode_label(&self) -> &'static str {
        match self.mode {
            AuthMode::Google { .. } => "google",
            #[cfg(feature = "dev-login")]
            AuthMode::DevTrust => "dev-trust",
        }
    }

    /// Google OAuth client `(id, secret)`, or `None` when this hub
    /// doesn't sign users in through Google.
    pub fn google_client(&self) -> Option<(&str, &str)> {
        match &self.mode {
            AuthMode::Google {
                client_id,
                client_secret,
            } => Some((client_id, client_secret)),
            #[cfg(feature = "dev-login")]
            AuthMode::DevTrust => None,
        }
    }

    /// True when `email` should bootstrap as admin on first Google
    /// sign-in. Used by session minting; subsequent admin appointments
    /// happen via `/_admin`.
    pub fn is_bootstrap_admin(&self, email: &str) -> bool {
        self.admin_emails.iter().any(|e| e == email)
    }
}

/// Pick the [`AuthMode`] from `ZIM_HUB_AUTH`. Anything but a honoured
/// `dev-trust` falls through to Google, which needs both creds — so a
/// default build ignores `ZIM_HUB_AUTH=dev-trust` and still demands them.
fn resolve_mode(
    auth: Option<&str>,
    host_name: &str,
    client_id: Option<String>,
    client_secret: Option<String>,
) -> Result<AuthMode, ConfigError> {
    match auth {
        #[cfg(feature = "dev-login")]
        Some("dev-trust") => {
            if !is_loopback(host_name) {
                return Err(ConfigError::DevTrustOffLoopback {
                    host_name: host_name.to_string(),
                });
            }
            Ok(AuthMode::DevTrust)
        }
        _ => {
            let _ = host_name; // only read by the dev-trust arm
            let (Some(client_id), Some(client_secret)) = (client_id, client_secret) else {
                return Err(ConfigError::AuthMissing);
            };
            Ok(AuthMode::Google {
                client_id,
                client_secret,
            })
        }
    }
}

/// True when `host_name` is a URL on this machine — the only place
/// dev-trust login is allowed to answer.
#[cfg(feature = "dev-login")]
fn is_loopback(host_name: &str) -> bool {
    url::Url::parse(host_name)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| matches!(h, "127.0.0.1" | "localhost" | "[::1]"))
        })
        .unwrap_or(false)
}

fn read_file_config(path: &Path) -> Result<FileConfig, ConfigError> {
    if !path.exists() {
        return Ok(FileConfig::default());
    }
    let raw = fs::read_to_string(path)?;
    Ok(toml::from_str(&raw)?)
}

fn write_file_config(path: &Path, file: &FileConfig) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let s = toml::to_string_pretty(file)?;
    fs::write(path, s)?;
    Ok(())
}

fn generate_secret() -> String {
    rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(64)
        .map(char::from)
        .collect()
}

/// Return the value of the first env var in `names` that's set and
/// non-empty. Used to accept both generic-style names and the
/// earlier `ZIM_HUB_` names with generic winning when both exist.
fn env_first(names: &[&str]) -> Option<String> {
    for n in names {
        if let Ok(v) = env::var(n) {
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds() -> (Option<String>, Option<String>) {
        (Some("client-id".into()), Some("client-secret".into()))
    }

    #[test]
    fn operator_with_google_creds_gets_google_mode() {
        let (id, secret) = creds();
        let mode = resolve_mode(None, "https://hub.example.com", id, secret).unwrap();
        assert!(matches!(mode, AuthMode::Google { .. }));
    }

    #[test]
    fn operator_without_google_creds_is_refused() {
        let err = resolve_mode(None, "http://127.0.0.1:17190", None, None).unwrap_err();
        assert!(matches!(err, ConfigError::AuthMissing));
    }

    // ── Default (release) build: dev-trust does not exist ──────────

    #[cfg(not(feature = "dev-login"))]
    #[test]
    fn release_build_ignores_dev_trust_and_still_demands_google_creds() {
        // Even on loopback, asking for dev-trust in a build without the
        // feature falls through to Google — and there are no creds.
        let err =
            resolve_mode(Some("dev-trust"), "http://127.0.0.1:17190", None, None).unwrap_err();
        assert!(matches!(err, ConfigError::AuthMissing));
    }

    #[cfg(not(feature = "dev-login"))]
    #[test]
    fn release_build_auth_mode_has_only_the_google_variant() {
        // Exhaustive with a single arm: adding an un-gated variant breaks
        // this match and forces a look at what a release build can do.
        let (id, secret) = creds();
        match resolve_mode(None, "https://hub.example.com", id, secret).unwrap() {
            AuthMode::Google { .. } => {}
        }
    }

    // ── dev-login build: trust mode, loopback only ─────────────────

    #[cfg(feature = "dev-login")]
    #[test]
    fn dev_on_loopback_gets_trust_mode_without_google_creds() {
        for host in [
            "http://127.0.0.1:17190",
            "http://localhost:17190",
            "http://[::1]:17190",
        ] {
            let mode = resolve_mode(Some("dev-trust"), host, None, None).unwrap();
            assert!(matches!(mode, AuthMode::DevTrust), "{host}");
        }
    }

    #[cfg(feature = "dev-login")]
    #[test]
    fn dev_trust_on_a_public_host_refuses_to_start() {
        let (id, secret) = creds();
        let err =
            resolve_mode(Some("dev-trust"), "https://hub.zim.krondor.org", id, secret).unwrap_err();
        assert!(matches!(err, ConfigError::DevTrustOffLoopback { .. }));
    }

    #[cfg(feature = "dev-login")]
    #[test]
    fn dev_build_without_dev_trust_still_runs_google() {
        let (id, secret) = creds();
        let mode = resolve_mode(None, "http://127.0.0.1:17190", id, secret).unwrap();
        assert!(matches!(mode, AuthMode::Google { .. }));
    }
}
