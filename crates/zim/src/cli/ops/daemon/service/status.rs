//! `zim daemon service status` — report install + run state.

use std::fmt;

use async_trait::async_trait;
use clap::Args;
use serde::Serialize;
use service_manager::{ServiceStatus, ServiceStatusCtx};

use crate::cli::op::Op;
use crate::cli::ui;

use super::{label, manager};

#[derive(Args, Debug, Clone)]
pub struct Status;

#[derive(Debug)]
pub struct StatusOutput(pub ServiceStatus);

/// Wire-shape for `--plain` JSON. `ServiceStatus` itself isn't
/// `Serialize`, so we project it into a tagged enum we control.
#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum StatusWire<'a> {
    NotInstalled,
    Running,
    Stopped { reason: Option<&'a str> },
}

impl Serialize for StatusOutput {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let wire = match &self.0 {
            ServiceStatus::NotInstalled => StatusWire::NotInstalled,
            ServiceStatus::Running => StatusWire::Running,
            ServiceStatus::Stopped(reason) => StatusWire::Stopped {
                reason: reason.as_deref(),
            },
        };
        wire.serialize(s)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StatusError {
    #[error("service manager: {0}")]
    Manager(std::io::Error),
    #[error("status: {0}")]
    Status(std::io::Error),
}

#[async_trait]
impl Op for Status {
    type Context = ();
    type Output = StatusOutput;
    type Error = StatusError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        let mgr = manager().map_err(StatusError::Manager)?;
        let s = mgr
            .status(ServiceStatusCtx { label: label() })
            .map_err(StatusError::Status)?;
        Ok(StatusOutput(s))
    }
}

impl fmt::Display for StatusOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let qn = label().to_qualified_name();
        match &self.0 {
            ServiceStatus::NotInstalled => write!(
                f,
                "{} — run `zim daemon service install`",
                ui::failure("not installed", &qn)
            ),
            ServiceStatus::Running => write!(f, "{}", ui::success("running", &qn)),
            ServiceStatus::Stopped(reason) => match reason {
                Some(r) => write!(f, "{} ({})", ui::warning("stopped", &qn), ui::dim(r)),
                None => write!(f, "{}", ui::warning("stopped", &qn)),
            },
        }
    }
}
