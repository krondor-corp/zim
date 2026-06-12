//! `zim daemon service stop` — stop the running daemon service.

use std::fmt;

use async_trait::async_trait;
use clap::Args;
use service_manager::ServiceStopCtx;

use crate::cli::op::Op;
use crate::cli::ui;

use super::{label, manager};

#[derive(Args, Debug, Clone)]
pub struct Stop;

#[derive(Debug, serde::Serialize)]
pub struct StopOutput {}

#[derive(Debug, thiserror::Error)]
pub enum StopError {
    #[error("service manager: {0}")]
    Manager(std::io::Error),
    #[error("stop: {0}")]
    Stop(std::io::Error),
}

#[async_trait]
impl Op for Stop {
    type Context = ();
    type Output = StopOutput;
    type Error = StopError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        let mgr = manager().map_err(StopError::Manager)?;
        mgr.stop(ServiceStopCtx { label: label() })
            .map_err(StopError::Stop)?;
        Ok(StopOutput {})
    }
}

impl fmt::Display for StopOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            ui::failure("stopped", &label().to_qualified_name())
        )
    }
}
