//! `zim daemon service start` — start the registered daemon service.
//!
//! Probes `/_status/livez` before delegating to the OS service
//! manager. If something is already listening on the configured API
//! port — the service itself (a launchd install starts at load),
//! another `zim daemon run` in the foreground — the daemon is already
//! up, so we report that and don't start a second one that would only
//! fail at bind time with EADDRINUSE.

use std::fmt;

use async_trait::async_trait;
use clap::Args;
use service_manager::ServiceStartCtx;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};

use super::{label, manager, probe};

#[derive(Args, Debug, Clone)]
pub struct Start;

#[derive(Debug, serde::Serialize)]
pub struct StartOutput {
    /// Address of the daemon that was already answering, if any.
    pub already_running: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error(transparent)]
    Context(#[from] ContextError),
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
        if probe::is_up(&ctx.client).await {
            return Ok(StartOutput {
                already_running: Some(ctx.client.remote().to_string()),
            });
        }

        start_service()?;
        Ok(StartOutput {
            already_running: None,
        })
    }
}

/// Ask the OS service manager to start the daemon.
pub fn start_service() -> Result<(), StartError> {
    let mgr = manager().map_err(StartError::Manager)?;
    mgr.start(ServiceStartCtx { label: label() })
        .map_err(StartError::Start)
}

impl fmt::Display for StartOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let qn = label().to_qualified_name();
        match &self.already_running {
            Some(addr) => write!(
                f,
                "{} ({})",
                ui::success("already running", &qn),
                ui::dim(addr)
            ),
            None => write!(f, "{}", ui::success("started", &qn)),
        }
    }
}
