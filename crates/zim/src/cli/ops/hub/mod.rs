//! `zim hub <subcommand>` — everything that touches a hub.
//!
//! The hub is **opt-in** (this whole module is behind the `hub` cargo
//! feature). A daemon that only syncs with your own devices over p2p
//! never needs it — the base `zim peers` commands manage the address
//! book directly. The hub matters when you want it to manage your device
//! roster: `zim hub login` pairs this daemon, and `zim hub peers sync`
//! pulls every device your account knows into the local address book.
//!
//! The HTTP client + wire types live in `zim_api::hub` (shared with the
//! hub's web SPA). This module only adds the daemon-side glue: reading
//! `$ZIM_HOME/hub-session.json` + identity to build a [`zim_api::hub::HubClient`].

pub mod login;
pub mod peers;

use std::fmt;

use async_trait::async_trait;
use clap::Subcommand;
use zim_crypto::PrivateKey;

use crate::cli::op::Op;
use crate::context::{paths, ContextError};

pub use login::Login;

#[derive(Subcommand, Debug, Clone)]
pub enum Hub {
    /// Pair this daemon with a hub via the device-code flow.
    Login(login::Login),
    /// Hub-mediated device/peer management.
    #[command(subcommand)]
    Peers(peers::HubPeers),
}

#[derive(Debug, serde::Serialize)]
#[serde(untagged)]
pub enum HubOutput {
    Login(login::LoginOutput),
    Peers(peers::HubPeersOutput),
}

#[derive(Debug, thiserror::Error)]
pub enum HubError {
    #[error(transparent)]
    Login(#[from] login::LoginError),
    #[error(transparent)]
    Peers(#[from] peers::HubPeersError),
}

#[async_trait]
impl Op for Hub {
    type Context = ();
    type Output = HubOutput;
    type Error = HubError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        Ok(match self {
            Hub::Login(c) => HubOutput::Login(c.run(c.build_context().await?).await?),
            Hub::Peers(c) => HubOutput::Peers(c.run(c.build_context().await?).await?),
        })
    }
}

impl fmt::Display for HubOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HubOutput::Login(o) => write!(f, "{o}"),
            HubOutput::Peers(o) => write!(f, "{o}"),
        }
    }
}

// ── Daemon-side session loader ───────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum HubSessionError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("not paired with a hub — run `zim hub login` first")]
    NoSession,
    #[error("identity: {0}")]
    Identity(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Api(#[from] zim_api::ApiError),
}

/// Load `$ZIM_HOME/hub-session.json` + identity into a ready
/// [`zim_api::hub::HubClient`]. Errors with [`HubSessionError::NoSession`]
/// when the daemon hasn't logged in — the signal to run `zim hub login`.
pub fn load_hub_client() -> Result<zim_api::hub::HubClient, HubSessionError> {
    let home = paths::home_dir(None)?;
    let session_path = paths::hub_session_file(&home);
    if !session_path.exists() {
        return Err(HubSessionError::NoSession);
    }
    let session: login::HubSession =
        serde_json::from_str(&std::fs::read_to_string(&session_path)?)?;
    let id_hex = std::fs::read_to_string(paths::identity_file(&home))?;
    let secret = PrivateKey::from_hex(id_hex.trim())
        .map_err(|e| HubSessionError::Identity(e.to_string()))?;
    Ok(zim_api::hub::HubClient::new(&session.hub_url, secret)?)
}
