//! `zim daemon <subcommand>` — the daemon process and its lifecycle.
//!
//! - `zim daemon run` — runs in the foreground until SIGINT/SIGTERM.
//! - `zim daemon service <op>` — register/start/stop/status/uninstall
//!   the daemon as a user-level service (launchd, systemd --user,
//!   sc.exe). See [`service`].
//!
//! The day-to-day daemon process lives in [`run`]; the service module
//! is just OS-integration plumbing on top of it.

use std::fmt;

use async_trait::async_trait;
use clap::Subcommand;

use crate::cli::op::Op;

pub mod logs;
pub mod run;
pub mod service;

#[derive(Subcommand, Debug, Clone)]
pub enum Daemon {
    /// Run the daemon in the foreground.
    Run(run::Run),
    /// Tail the daemon's log file at `$ZIM_HOME/state/daemon.log`.
    Logs(logs::Logs),
    /// Manage the daemon as an OS service.
    #[command(subcommand)]
    Service(service::Service),
}

#[derive(Debug, serde::Serialize)]
#[serde(untagged)]
pub enum DaemonOutput {
    Run(run::RunOutput),
    Logs(logs::LogsOutput),
    Service(service::ServiceOutput),
}

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error(transparent)]
    Run(#[from] run::RunError),
    #[error(transparent)]
    Logs(#[from] logs::LogsError),
    #[error(transparent)]
    Service(#[from] service::ServiceError),
}

#[async_trait]
impl Op for Daemon {
    type Context = ();
    type Output = DaemonOutput;
    type Error = DaemonError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        Ok(match self {
            Daemon::Run(c) => DaemonOutput::Run(c.run(c.build_context().await?).await?),
            Daemon::Logs(c) => DaemonOutput::Logs(c.run(c.build_context().await?).await?),
            Daemon::Service(c) => DaemonOutput::Service(c.run(c.build_context().await?).await?),
        })
    }
}

impl fmt::Display for DaemonOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DaemonOutput::Run(o) => write!(f, "{o}"),
            DaemonOutput::Logs(o) => write!(f, "{o}"),
            DaemonOutput::Service(o) => write!(f, "{o}"),
        }
    }
}
