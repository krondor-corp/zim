//! `zim daemon service start` — start the registered daemon service.
//!
//! Probes `/_status/livez` before delegating to the OS service
//! manager. If something is already listening on the configured API
//! port — another `zim daemon run` in the foreground, a stale
//! process, a different program — we abort rather than letting the
//! second daemon fail at bind time with EADDRINUSE.

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use clap::Args;
use service_manager::ServiceStartCtx;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::daemon::health::liveness::LivezRequest;

use super::{label, manager};

/// Same short timeout the `health` dashboard uses for its livez
/// probe. If the daemon doesn't answer within this window we treat
/// the port as free and proceed.
const PROBE_TIMEOUT: Duration = Duration::from_millis(750);

#[derive(Args, Debug, Clone)]
pub struct Start;

#[derive(Debug, serde::Serialize)]
pub struct StartOutput {}

#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error("already running at {0}")]
    AlreadyRunning(String),
    #[error("service manager: {0}")]
    Manager(std::io::Error),
    #[error("start: {0}")]
    Start(std::io::Error),
}

#[async_trait]
impl Op for Start {
    type Context = ApiContext;
    type Output = StartOutput;
    type Error = StartError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        let already_up = matches!(
            tokio::time::timeout(PROBE_TIMEOUT, ctx.client.call(LivezRequest {})).await,
            Ok(Ok(_))
        );
        if already_up {
            return Err(StartError::AlreadyRunning(ctx.client.remote().to_string()));
        }

        let mgr = manager().map_err(StartError::Manager)?;
        mgr.start(ServiceStartCtx { label: label() })
            .map_err(StartError::Start)?;
        Ok(StartOutput {})
    }
}

impl fmt::Display for StartOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            ui::success("started", &label().to_qualified_name())
        )
    }
}
