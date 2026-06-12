//! `zim health` — peer status dashboard.
//!
//! Aggregates the local peer identity, whether the daemon is running,
//! and (when it lands) hub connectivity. The dashboard never errors:
//! a missing identity file or a downed daemon are normal states it
//! reports.
//!
//! The daemon-side `/_status/{livez,readyz,version,identity}` HTTP
//! endpoints stay live for external monitoring; we just don't expose
//! a CLI surface for them.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{paths, ApiContext, ContextError};
use crate::http_server::api::client::ApiError;
use crate::http_server::health::liveness::LivezRequest;

/// Short timeout for the livez probe — we want a fast "is it up?"
/// answer, not to hang waiting on a stuck daemon.
const PROBE_TIMEOUT: Duration = Duration::from_millis(750);

#[derive(Args, Debug, Clone)]
pub struct Health;

#[derive(Debug, serde::Serialize)]
pub struct HealthOutput {
    /// Resolved data directory — handy for `dev shell` setups where
    /// you want to confirm which peer's home this CLI is talking to.
    pub home: PathBuf,
    /// Pubkey hex read from the local `identity.key`, or `None` if
    /// `zim init` hasn't been run yet.
    pub peer_id: Option<String>,
    /// Daemon endpoint we probed.
    pub endpoint: String,
    /// True if `/_status/livez` answered within `PROBE_TIMEOUT`.
    pub daemon_up: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum HealthError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Api(#[from] ApiError),
}

#[async_trait]
impl Op for Health {
    type Context = ApiContext;
    type Output = HealthOutput;
    type Error = HealthError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let peer_id = read_local_identity(&ctx.home).await;
        let endpoint = ctx.client.remote().to_string();
        let daemon_up = matches!(
            tokio::time::timeout(PROBE_TIMEOUT, ctx.client.call(LivezRequest {})).await,
            Ok(Ok(_))
        );
        Ok(HealthOutput {
            home: ctx.home,
            peer_id,
            endpoint,
            daemon_up,
        })
    }
}

/// Read the public hex from `<home>/identity.key`. The file stores
/// the *private* key hex; derive the public key from it. Returns
/// `None` if the file is missing or unreadable — both are "not
/// initialised yet" from the dashboard's perspective.
async fn read_local_identity(home: &std::path::Path) -> Option<String> {
    use zim_crypto::PrivateKey;
    let path = paths::identity_file(home);
    let hex = tokio::fs::read_to_string(&path).await.ok()?;
    let secret = PrivateKey::from_hex(hex.trim()).ok()?;
    Some(secret.public().to_hex())
}

impl fmt::Display for HealthOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "home    {}", ui::ident(self.home.display().to_string()))?;

        let peer = match &self.peer_id {
            Some(hex) => ui::ident(hex),
            None => format!("{} {}", ui::dim("—"), ui::dim("run `zim init`")),
        };
        writeln!(f, "peer    {peer}")?;

        let daemon = if self.daemon_up {
            format!(
                "{} {}   {}",
                ui::SUCCESS,
                "running",
                ui::dim(&self.endpoint)
            )
        } else {
            format!(
                "{} {}   {}",
                ui::FAILURE,
                "not running",
                ui::dim("try `zim daemon run` or `zim daemon service start`")
            )
        };
        writeln!(f, "daemon  {daemon}")?;

        write!(f, "hub     {}", ui::dim("—"))
    }
}
