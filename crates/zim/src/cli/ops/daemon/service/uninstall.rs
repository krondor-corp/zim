//! `zim daemon service uninstall` — unregister the daemon service.

use std::fmt;

use async_trait::async_trait;
use clap::Args;
use service_manager::ServiceUninstallCtx;

use crate::cli::op::Op;
use crate::cli::ui;

use super::{label, manager};

#[derive(Args, Debug, Clone)]
pub struct Uninstall;

#[derive(Debug, serde::Serialize)]
pub struct UninstallOutput {}

#[derive(Debug, thiserror::Error)]
pub enum UninstallError {
    #[error("service manager: {0}")]
    Manager(std::io::Error),
    #[error("uninstall: {0}")]
    Uninstall(std::io::Error),
}

#[async_trait]
impl Op for Uninstall {
    type Context = ();
    type Output = UninstallOutput;
    type Error = UninstallError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        let mgr = manager().map_err(UninstallError::Manager)?;
        mgr.uninstall(ServiceUninstallCtx { label: label() })
            .map_err(UninstallError::Uninstall)?;
        Ok(UninstallOutput {})
    }
}

impl fmt::Display for UninstallOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            ui::failure("uninstalled", &label().to_qualified_name())
        )
    }
}
