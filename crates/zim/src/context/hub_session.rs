//! `$ZIM_HOME/hub-session.json` — which hub this daemon is paired with.
//!
//! Written by `zim hub login`; read by the `zim hub` commands and by the
//! daemon itself (a paired daemon seals new vaults to the account's whole
//! device roster). Ungated by the `hub` feature: a daemon built without
//! the subcommand still honours a session written by one built with it.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::paths;

/// What we persist at `$ZIM_HOME/hub-session.json`. There's no secret
/// material here — `identity.key` is the only thing the daemon can sign
/// with, and JWTs are minted on the fly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubSession {
    pub hub_url: String,
    pub enrolled_pubkey: String,
    pub enrolled_at: String,
}

#[derive(Debug, thiserror::Error)]
pub enum HubSessionReadError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("hub-session.json: {0}")]
    Json(#[from] serde_json::Error),
}

impl HubSession {
    /// The session under `home`, or `None` when this daemon isn't paired.
    pub fn load(home: &Path) -> Result<Option<Self>, HubSessionReadError> {
        let path = paths::hub_session_file(home);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&std::fs::read_to_string(
            &path,
        )?)?))
    }
}
