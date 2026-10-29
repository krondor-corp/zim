//! Device-code login: pair this daemon's identity with a hub.
//!
//! A two-phase, non-printing flow so any front end (the `zim hub login`
//! CLI, a desktop app) can show the pairing instructions its own way:
//!
//! 1. [`start`] loads `identity.key` from `$ZIM_HOME` and opens a
//!    device-code grant, committing to our pubkey. The returned
//!    [`PendingLogin`] carries what the user needs to approve it — the
//!    verification URL, the code, and our pubkey.
//! 2. [`PendingLogin::complete`] polls until the user approves (or the
//!    code expires / the timeout runs out), then writes
//!    `$ZIM_HOME/hub-session.json` and, unless told not to, upserts the
//!    hub into the peer book.
//!
//! The poll body is signed over `code || pubkey`, so the poll that sees
//! the approval *is* the enrollment — no separate round-trip and no
//! long-lived bearer token to persist. Afterwards every hub call is
//! authenticated by a short-lived JWT minted from the same identity key
//! (see [`zim_api::hub::jwt::mint`]).

use std::path::{Path, PathBuf};
use std::time::Duration;

use zim_api::hub::{DeviceCodeStart, DeviceCodeStartRequest};
use zim_api::{Client, Url};
use zim_crypto::PrivateKey;

use crate::context::paths;

mod error;
mod persist;
mod poll;
#[cfg(test)]
mod tests;

pub use error::LoginError;

/// Per-request HTTP timeout for every call to the hub.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Open a device-code grant on `hub_url` for the identity under `home`.
///
/// `label` is shown on the hub's approve page; `None` uses the machine's
/// hostname. Fails before contacting the hub when there's no identity to
/// sign with.
pub async fn start(
    home: &Path,
    hub_url: &str,
    label: Option<&str>,
) -> Result<PendingLogin, LoginError> {
    let hub_url = hub_url.trim_end_matches('/').to_string();
    let label = label
        .map(str::to_string)
        .or_else(|| hostname::get().ok().and_then(|s| s.into_string().ok()))
        .unwrap_or_else(|| "daemon".to_string());

    // Load identity first — fail before minting a server-side grant if
    // we can't sign.
    let id_path = paths::identity_file(home);
    let identity_hex = tokio::fs::read_to_string(&id_path).await.map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!(
                "no identity at {} — run `zim init` first",
                id_path.display()
            ),
        )
    })?;
    let secret = PrivateKey::from_hex(identity_hex.trim())
        .map_err(|e| LoginError::Identity(e.to_string()))?;
    let pubkey_hex = secret.public().to_hex();

    let base =
        Url::parse(&hub_url).map_err(|e| LoginError::BadHubUrl(format!("{hub_url}: {e}")))?;
    let http = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| LoginError::Hub(e.to_string()))?;
    let client = Client::with_http(&base, http);

    let grant = client
        .call(DeviceCodeStartRequest {
            pubkey: pubkey_hex.clone(),
            label,
        })
        .await?;

    Ok(PendingLogin {
        home: home.to_path_buf(),
        hub_url,
        client,
        secret,
        pubkey_hex,
        grant,
    })
}

/// A device-code grant waiting for the user to approve it on the hub.
/// Show [`verification_url`](Self::verification_url), [`code`](Self::code)
/// and [`pubkey_hex`](Self::pubkey_hex), then [`complete`](Self::complete).
pub struct PendingLogin {
    home: PathBuf,
    hub_url: String,
    client: Client,
    secret: PrivateKey,
    pubkey_hex: String,
    grant: DeviceCodeStart,
}

/// How [`PendingLogin::complete`] finishes the login.
#[derive(Debug, Clone)]
pub struct CompleteOptions {
    /// Give up polling after this long.
    pub timeout: Duration,
    /// Nickname to upsert the hub under in the local peer book (an
    /// existing entry is overwritten). `None` leaves the peer book
    /// untouched.
    pub peer_nick: Option<String>,
}

/// A finished login: what was enrolled and what was written.
#[derive(Debug, serde::Serialize)]
pub struct LoginOutput {
    pub hub_url: String,
    pub enrolled_pubkey: String,
    pub session_path: PathBuf,
    /// The hub's DID (from its own `/.well-known/did.json`) as written to
    /// the peer book. `None` when the peer book was skipped.
    pub hub_did: Option<String>,
    /// The peer-book nickname used. `None` when the peer book was skipped.
    pub hub_nick: Option<String>,
}

impl PendingLogin {
    /* Getters */

    /// The hub's base URL (trailing slash stripped).
    pub fn hub_url(&self) -> &str {
        &self.hub_url
    }

    /// The hub page the user opens to approve this login.
    pub fn verification_url(&self) -> &str {
        &self.grant.verification_url
    }

    /// The code the user checks on the approve page.
    pub fn code(&self) -> &str {
        &self.grant.code
    }

    /// This daemon's identity pubkey (hex) — the approve page shows it
    /// too, for the user to match.
    pub fn pubkey_hex(&self) -> &str {
        &self.pubkey_hex
    }

    /// When the code expires, as the hub reports it.
    pub fn expires_at(&self) -> &str {
        &self.grant.expires_at
    }

    /// The hub's suggested poll interval, in seconds.
    pub fn poll_interval_secs(&self) -> u64 {
        self.grant.poll_interval_secs
    }

    /* Completion */

    /// Poll until the user approves, then persist the session and
    /// (per `opts`) the peer-book entry.
    pub async fn complete(self, opts: CompleteOptions) -> Result<LoginOutput, LoginError> {
        poll::until_enrolled(
            &self.client,
            &self.grant.code,
            &self.secret,
            Duration::from_secs(self.grant.poll_interval_secs.max(1)),
            opts.timeout,
        )
        .await?;

        let session_path =
            persist::write_session(&self.home, &self.hub_url, &self.pubkey_hex).await?;

        let hub_did = match &opts.peer_nick {
            Some(nick) => {
                Some(persist::add_hub_to_peer_book(&self.client, &self.home, nick).await?)
            }
            None => None,
        };

        Ok(LoginOutput {
            hub_url: self.hub_url,
            enrolled_pubkey: self.pubkey_hex,
            session_path,
            hub_did,
            hub_nick: opts.peer_nick,
        })
    }
}
