//! `zim daemon service restart` — stop the daemon, wait for it to
//! drain, start it again. Also what `zim update` uses to put a freshly
//! swapped binary into service.

use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;
use crate::context::{ApiContext, ContextError};
use crate::daemon::api::client::ApiClient;

use super::label;
use super::start::{start_service, StartError};
use super::stop::{stop_service, StopError};

#[derive(Args, Debug, Clone)]
pub struct Restart;

#[derive(Debug, serde::Serialize)]
pub struct RestartOutput {}

#[derive(Debug, thiserror::Error)]
pub enum RestartError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    Stop(#[from] StopError),
    #[error(transparent)]
    Start(#[from] StartError),
}

#[async_trait]
impl Op for Restart {
    type Context = ApiContext;
    type Output = RestartOutput;
    type Error = RestartError;

    async fn build_context(&self) -> Result<ApiContext, Self::Error> {
        Ok(ApiContext::build(None)?)
    }

    async fn run(&self, ctx: ApiContext) -> Result<Self::Output, Self::Error> {
        restart_service(&ctx.client).await?;
        Ok(RestartOutput {})
    }
}

/// Stop the service, wait until the daemon is gone, start it again.
/// Waiting matters: launchd ignores a start for a job that is still
/// draining, which would leave the daemon down.
pub async fn restart_service(client: &ApiClient) -> Result<(), RestartError> {
    stop_service(client).await?;
    start_service()?;
    Ok(())
}

impl fmt::Display for RestartOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            ui::success("restarted", &label().to_qualified_name())
        )
    }
}
