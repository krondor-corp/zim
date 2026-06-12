//! `zim daemon service <subcommand>` — service-manager wrappers
//! for the long-running daemon process.
//!
//! Defaults to the **user** service level (launchd `~/Library/
//! LaunchAgents`, systemd `--user`) — no root required. The daemon
//! itself lives one level up at `zim daemon run`; this module is
//! just the registration / start / stop plumbing.

use std::fmt;

use async_trait::async_trait;
use clap::Subcommand;
use service_manager::{ServiceLabel, ServiceLevel, ServiceManager};

use crate::cli::op::Op;

pub mod install;
pub mod start;
pub mod status;
pub mod stop;
pub mod uninstall;

#[derive(Subcommand, Debug, Clone)]
pub enum Service {
    /// Register the daemon with the OS service manager.
    Install(install::Install),
    /// Start the registered service.
    Start(start::Start),
    /// Stop the running service.
    Stop(stop::Stop),
    /// Show whether the service is installed / running.
    Status(status::Status),
    /// Unregister the service.
    Uninstall(uninstall::Uninstall),
}

#[derive(Debug, serde::Serialize)]
#[serde(untagged)]
pub enum ServiceOutput {
    Install(install::InstallOutput),
    Start(start::StartOutput),
    Stop(stop::StopOutput),
    Status(status::StatusOutput),
    Uninstall(uninstall::UninstallOutput),
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error(transparent)]
    Install(#[from] install::InstallError),
    #[error(transparent)]
    Start(#[from] start::StartError),
    #[error(transparent)]
    Stop(#[from] stop::StopError),
    #[error(transparent)]
    Status(#[from] status::StatusError),
    #[error(transparent)]
    Uninstall(#[from] uninstall::UninstallError),
}

#[async_trait]
impl Op for Service {
    type Context = ();
    type Output = ServiceOutput;
    type Error = ServiceError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        Ok(match self {
            Service::Install(c) => ServiceOutput::Install(c.run(c.build_context().await?).await?),
            Service::Start(c) => ServiceOutput::Start(c.run(c.build_context().await?).await?),
            Service::Stop(c) => ServiceOutput::Stop(c.run(c.build_context().await?).await?),
            Service::Status(c) => ServiceOutput::Status(c.run(c.build_context().await?).await?),
            Service::Uninstall(c) => {
                ServiceOutput::Uninstall(c.run(c.build_context().await?).await?)
            }
        })
    }
}

impl fmt::Display for ServiceOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceOutput::Install(o) => write!(f, "{o}"),
            ServiceOutput::Start(o) => write!(f, "{o}"),
            ServiceOutput::Stop(o) => write!(f, "{o}"),
            ServiceOutput::Status(o) => write!(f, "{o}"),
            ServiceOutput::Uninstall(o) => write!(f, "{o}"),
        }
    }
}

/// The canonical label for the zim daemon. Renders as
/// `org.zim.daemon` on launchd and `zim-daemon.service` on systemd.
pub fn label() -> ServiceLabel {
    ServiceLabel {
        qualifier: Some("org".into()),
        organization: Some("zim".into()),
        application: "daemon".into(),
    }
}

/// Native, user-level service manager for the current OS.
pub fn manager() -> std::io::Result<Box<dyn ServiceManager>> {
    let mut m = <dyn ServiceManager>::native()?;
    m.set_level(ServiceLevel::User)?;
    Ok(m)
}
