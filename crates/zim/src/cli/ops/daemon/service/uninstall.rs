//! `zim daemon service uninstall` — unregister the daemon service.

use std::fmt;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::cli::ui;

use super::{backend, label};

#[derive(Args, Debug, Clone)]
pub struct Uninstall;

#[derive(Debug, serde::Serialize)]
pub struct UninstallOutput {}

#[derive(Debug, thiserror::Error)]
pub enum UninstallError {
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
        backend::uninstall().map_err(UninstallError::Uninstall)?;
        Ok(UninstallOutput {})
    }
}

impl fmt::Display for UninstallOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            ui::success("uninstalled", &label().to_qualified_name())
        )
    }
}
