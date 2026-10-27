//! `zim daemon service stop` — stop the running daemon service.
//!
//! The service manager's stop is fire-and-forget (launchd's
//! `launchctl stop` returns before the daemon has drained), so after
//! asking we poll `/_status/livez` until the daemon is actually gone.
//! Installs from older `zim` versions registered with launchd's bare
//! `KeepAlive`, which respawns the daemon after any exit — we refuse
//! those up front rather than report a stop that won't stick.

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use clap::Args;
use service_manager::ServiceStopCtx;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::daemon::api::client::ApiClient;

use super::{label, launchd, manager, probe};

/// How long to wait for the daemon to drain and exit. Comfortably
/// above its graceful-shutdown window.
const STOP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Args, Debug, Clone)]
pub struct Stop;

#[derive(Debug, serde::Serialize)]
pub struct StopOutput {}

#[derive(Debug, thiserror::Error)]
pub enum StopError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error("read launchd plist: {0}")]
    Plist(std::io::Error),
    #[error(
        "registered with launchd KeepAlive, which respawns the daemon after any stop — \
         run `zim daemon service install` to re-register, or `zim daemon service uninstall`"
    )]
    RespawnsOnStop,
    #[error("service manager: {0}")]
    Manager(std::io::Error),
    #[error("stop: {0}")]
    Stop(std::io::Error),
    #[error("still answering at {0} — a foreground `zim daemon run`?")]
    StillRunning(String),
}

#[async_trait]
impl Op for Stop {
    type Context = ApiContext;
    type Output = StopOutput;
    type Error = StopError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        stop_service(&ctx.client).await?;
        Ok(StopOutput {})
    }
}

/// Stop the service and wait until the daemon stops answering.
pub async fn stop_service(client: &ApiClient) -> Result<(), StopError> {
    let qn = label().to_qualified_name();
    if launchd::respawns_on_stop(&qn).map_err(StopError::Plist)? {
        return Err(StopError::RespawnsOnStop);
    }

    // The manager isn't `Send`; drop it before awaiting.
    manager()
        .map_err(StopError::Manager)?
        .stop(ServiceStopCtx { label: label() })
        .map_err(StopError::Stop)?;

    if !probe::wait_until_down(client, STOP_TIMEOUT).await {
        return Err(StopError::StillRunning(client.remote().to_string()));
    }
    Ok(())
}

impl fmt::Display for StopOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            ui::success("stopped", &label().to_qualified_name())
        )
    }
}
