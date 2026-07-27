//! `zim hub login --hub <url>` — pair this daemon with a hub.
//!
//! Runs the device-code flow against the hub's
//! `/api/v0/auth/device-code/*` endpoints. The daemon commits to
//! its pubkey at start-time and signs the code+pubkey at poll-time,
//! so the poll-when-approved response IS the enrollment — no
//! separate `/api/v0/devices/self` round-trip and no long-lived
//! bearer token to persist.
//!
//! The terminal prints a URL + a code; the user opens the URL in a
//! browser, signs in if needed, verifies that the displayed pubkey
//! matches what the daemon printed, clicks Approve. We poll in a
//! loop and finish enrollment without further input.
//!
//! On success, all future hub calls are authenticated by signing a
//! short-lived JWT with the same identity key (see
//! [`zim_api::hub::jwt::mint`]).

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use clap::Args;
use serde::{Deserialize, Serialize};
use zim_crypto::PrivateKey;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{paths, ContextError};

/// Default `--hub` target. Debug builds point at the local dev hub
/// (`make hub` serves `127.0.0.1:8080`); release points at the hosted
/// hub. Passing `--hub` explicitly always wins.
pub const DEFAULT_HUB: &str = if cfg!(debug_assertions) {
    "http://127.0.0.1:8080"
} else {
    "https://zim.krondor.org"
};

#[derive(Args, Debug, Clone)]
pub struct Login {
    /// Hub base URL, e.g. `https://hub.example.com`. No trailing
    /// slash; we strip one if you pass one. Defaults to the local dev
    /// hub in debug builds, the hosted hub in release.
    #[arg(long, default_value_t = DEFAULT_HUB.to_string())]
    pub hub: String,

    /// Override the device label shown on the approval page. Defaults
    /// to the machine's hostname.
    #[arg(long)]
    pub label: Option<String>,

    /// Nickname for the hub in the local peer book. Existing entry
    /// is overwritten so a re-login redirects future `relays add
    /// <nick>` calls.
    #[arg(long, default_value = "hub")]
    pub nick: String,

    /// Skip the peer-book update step. Useful if you only want to
    /// enroll without affecting how vaults route mirrors.
    #[arg(long)]
    pub no_peer_add: bool,

    /// Cap on how long we'll poll before giving up. Default 10 min,
    /// matches the hub's grant TTL.
    #[arg(long, default_value_t = 600)]
    pub timeout_secs: u64,
}

/// What we persist at `$ZIM_HOME/hub-session.json`. The daemon
/// reads it on startup to know which hub it's paired with. There's
/// no secret material here — `identity.key` is the only thing the
/// daemon can sign with, and JWTs are minted on the fly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubSession {
    pub hub_url: String,
    pub enrolled_pubkey: String,
    pub enrolled_at: String,
}

#[derive(Debug, serde::Serialize)]
pub struct LoginOutput {
    pub hub_url: String,
    pub enrolled_pubkey: String,
    pub session_path: PathBuf,
    /// `did:web:<host>` derived from the hub URL and written to the
    /// local peer book under `--nick`. `None` when `--no-peer-add`
    /// was set.
    pub hub_did: Option<String>,
    pub hub_nick: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LoginError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("identity decode: {0}")]
    Identity(String),
    #[error("hub request: {0}")]
    Hub(String),
    #[error("hub returned {0}: {1}")]
    HubStatus(reqwest::StatusCode, String),
    #[error("device code expired before approval")]
    Expired,
    #[error("login timed out after {0}s")]
    Timeout(u64),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("peer book: {0}")]
    PeerBook(String),
    #[error("invalid hub url: {0}")]
    BadHubUrl(String),
}

#[async_trait]
impl Op for Login {
    type Context = ();
    type Output = LoginOutput;
    type Error = LoginError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        let hub_url = self.hub.trim_end_matches('/').to_string();
        let label = self
            .label
            .clone()
            .or_else(|| hostname::get().ok().and_then(|s| s.into_string().ok()))
            .unwrap_or_else(|| "daemon".to_string());

        // Load identity early — fail before minting a server-side
        // grant if we can't sign.
        let home = paths::home_dir(None)?;
        let id_path = paths::identity_file(&home);
        let identity_hex = tokio::fs::read_to_string(&id_path).await.map_err(|e| {
            LoginError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "no identity at {} — run `zim init` first",
                    id_path.display()
                ),
            ))
        })?;
        let secret = PrivateKey::from_hex(identity_hex.trim())
            .map_err(|e| LoginError::Identity(e.to_string()))?;
        let pubkey_hex = secret.public().to_hex();

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| LoginError::Hub(e.to_string()))?;

        // 1. Start the device-code flow. We commit to our pubkey
        //    here so the approve page can render it for the user.
        let start = start_device_code(&client, &hub_url, &pubkey_hex, &label).await?;
        print_pair_instructions(&start, &pubkey_hex);

        // 2. Poll until the grant is approved or the code expires.
        //    The poll body carries an ed25519 signature over
        //    code_bytes || pubkey_bytes — the hub atomically
        //    enrolls when both the user-side approval AND this
        //    possession proof line up.
        poll_until_enrolled(
            &client,
            &hub_url,
            &start.code,
            &secret,
            &pubkey_hex,
            start.poll_interval_secs.max(1),
            self.timeout_secs,
        )
        .await?;

        // 3. Persist the session marker.
        let session = HubSession {
            hub_url: hub_url.clone(),
            enrolled_pubkey: pubkey_hex.clone(),
            enrolled_at: chrono::Utc::now().to_rfc3339(),
        };
        let session_path = paths::hub_session_file(&home);
        let serialized = serde_json::to_string_pretty(&session)?;
        tokio::fs::write(&session_path, serialized).await?;
        #[cfg(unix)]
        {
            // Mode 0600 is overcautious now that the file carries no
            // secrets, but it keeps the contract that anything under
            // $ZIM_HOME is owner-only.
            use std::os::unix::fs::PermissionsExt;
            let _ =
                tokio::fs::set_permissions(&session_path, std::fs::Permissions::from_mode(0o600))
                    .await;
        }

        // 4. Add the hub to the local peer book so the daemon can
        //    resolve `relays add <nick>` later.
        //
        //    Read the DID off the hub's own /.well-known/did.json
        //    rather than deriving it from `--hub`. A user typing
        //    `--hub http://localhost:8080` against a hub configured
        //    with `ZIM_HUB_HOST=127.0.0.1:8080` would otherwise get
        //    `did:web:localhost%3A8080` in their peer book, which
        //    then fails to resolve because the document at that URL
        //    self-identifies as `did:web:127.0.0.1%3A8080` and the
        //    resolver insists the two match. The hub is the
        //    authority on its own DID.
        let (hub_did, hub_nick) = if self.no_peer_add {
            (None, None)
        } else {
            let did = fetch_hub_did(&client, &hub_url).await?;
            let identity =
                zim_did::Did::parse(&did).map_err(|e| LoginError::PeerBook(e.to_string()))?;
            let store = zim_peer::SqlitePeerStore::open(&paths::log_file(&home))
                .map_err(|e| LoginError::PeerBook(e.to_string()))?;
            // The hub is a relay endpoint, not a vault shareholder, so it's
            // an *untrusted* contact — named for `relays add <nick>`, never
            // auto-shared into your vaults.
            zim_peer::PeerStore::upsert(
                &store,
                &self.nick,
                identity,
                false,
                Some("zim hub".to_string()),
            )
            .await
            .map_err(|e| LoginError::PeerBook(e.to_string()))?;
            (Some(did), Some(self.nick.clone()))
        };

        Ok(LoginOutput {
            hub_url,
            enrolled_pubkey: pubkey_hex,
            session_path,
            hub_did,
            hub_nick,
        })
    }
}

/// Fetch the hub's `/.well-known/did.json` and return the `id`
/// field. This is the DID the rest of the resolver chain will
/// compare against when it walks the DID later, so writing it
/// verbatim to the peer book guarantees alignment regardless of
/// whether the user typed `localhost`, `127.0.0.1`, or a hostname.
async fn fetch_hub_did(client: &reqwest::Client, hub_url: &str) -> Result<String, LoginError> {
    #[derive(Deserialize)]
    struct DidDoc {
        id: String,
    }
    let url = format!("{}/.well-known/did.json", hub_url.trim_end_matches('/'));
    let doc: DidDoc = client
        .get(&url)
        .send()
        .await
        .map_err(|e| LoginError::Hub(format!("fetch {url}: {e}")))?
        .error_for_status()
        .map_err(|e| LoginError::Hub(format!("fetch {url}: {e}")))?
        .json()
        .await
        .map_err(|e| LoginError::Hub(format!("parse {url}: {e}")))?;
    if !doc.id.starts_with("did:web:") {
        return Err(LoginError::BadHubUrl(format!(
            "hub DID document `id` is not did:web: {}",
            doc.id
        )));
    }
    Ok(doc.id)
}

#[derive(Debug, Deserialize)]
struct StartResponse {
    code: String,
    verification_url: String,
    expires_at: String,
    poll_interval_secs: u64,
}

async fn start_device_code(
    client: &reqwest::Client,
    hub_url: &str,
    pubkey_hex: &str,
    label: &str,
) -> Result<StartResponse, LoginError> {
    let res = client
        .post(format!("{hub_url}/api/v0/auth/device-code/start"))
        .json(&serde_json::json!({ "pubkey": pubkey_hex, "label": label }))
        .send()
        .await
        .map_err(|e| LoginError::Hub(e.to_string()))?;
    let status = res.status();
    let body = res
        .text()
        .await
        .map_err(|e| LoginError::Hub(e.to_string()))?;
    if !status.is_success() {
        return Err(LoginError::HubStatus(status, body));
    }
    Ok(serde_json::from_str(&body)?)
}

fn print_pair_instructions(start: &StartResponse, pubkey_hex: &str) {
    eprintln!();
    eprintln!("  Open this URL in your browser:");
    eprintln!("    {}", ui::ident(&start.verification_url));
    eprintln!();
    eprintln!("  Verify the code on the approve page matches:");
    eprintln!("    {}", ui::ident(&start.code));
    eprintln!();
    eprintln!("  …and the identity key matches:");
    eprintln!("    {}", ui::ident(pubkey_hex));
    eprintln!();
    eprintln!(
        "  Code expires {} · polling every {}s …",
        ui::dim(&start.expires_at),
        start.poll_interval_secs
    );
}

async fn poll_until_enrolled(
    client: &reqwest::Client,
    hub_url: &str,
    code: &str,
    secret: &PrivateKey,
    pubkey_hex: &str,
    interval_secs: u64,
    timeout_secs: u64,
) -> Result<(), LoginError> {
    let interval = Duration::from_secs(interval_secs);
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);

    // Possession-proof signature is fixed for the lifetime of this
    // poll loop — sign once, send it on every request. The signing
    // payload is `code_bytes || pubkey_bytes`. The hub will reject
    // any poll that doesn't carry it once the grant is approved.
    let mut payload = Vec::with_capacity(code.len() + 32);
    payload.extend_from_slice(code.as_bytes());
    payload.extend_from_slice(&secret.public().to_bytes());
    let sig_hex = hex::encode(secret.sign(&payload).to_bytes());

    loop {
        if std::time::Instant::now() >= deadline {
            return Err(LoginError::Timeout(timeout_secs));
        }
        let res = client
            .post(format!("{hub_url}/api/v0/auth/device-code/poll"))
            .json(&serde_json::json!({
                "code": code,
                "signature": sig_hex,
            }))
            .send()
            .await
            .map_err(|e| LoginError::Hub(e.to_string()))?;
        match res.status().as_u16() {
            200 => {
                // Sanity: the hub echoes the pubkey it enrolled.
                // Catch a misaddressed login that somehow targeted
                // the wrong daemon's grant.
                #[derive(Deserialize)]
                struct PollOk {
                    pubkey: String,
                }
                let body: PollOk = res
                    .json()
                    .await
                    .map_err(|e| LoginError::Hub(e.to_string()))?;
                if body.pubkey != pubkey_hex {
                    return Err(LoginError::Hub(format!(
                        "hub enrolled a different pubkey ({} vs ours {})",
                        body.pubkey, pubkey_hex
                    )));
                }
                return Ok(());
            }
            202 => {
                // Pending approval. Keep waiting.
                tokio::time::sleep(interval).await;
            }
            410 => return Err(LoginError::Expired),
            other => {
                let body = res.text().await.unwrap_or_default();
                return Err(LoginError::HubStatus(
                    reqwest::StatusCode::from_u16(other)
                        .unwrap_or(reqwest::StatusCode::BAD_GATEWAY),
                    body,
                ));
            }
        }
    }
}

impl fmt::Display for LoginOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} {}",
            ui::success("logged in", ""),
            ui::dim(&self.hub_url)
        )?;
        writeln!(f, "  pubkey:  {}", ui::ident(&self.enrolled_pubkey))?;
        writeln!(
            f,
            "  session: {}",
            ui::dim(self.session_path.display().to_string())
        )?;
        match (&self.hub_nick, &self.hub_did) {
            (Some(nick), Some(did)) => {
                writeln!(
                    f,
                    "  peer:    {} {}",
                    ui::ident(nick),
                    ui::dim(format!("→ {did}"))
                )?;
                let _ = nick;
                write!(f, "  next:    {}", ui::ident("zim hub peers sync"))
            }
            _ => write!(f, "  peer:    {}", ui::dim("(skipped via --no-peer-add)")),
        }
    }
}
