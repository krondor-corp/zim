//! `zim daemon service install` — register the daemon as a user service.

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use async_trait::async_trait;
use clap::Args;
use service_manager::ServiceInstallCtx;

use crate::cli::op::Op;
use crate::cli::ui;

use super::{label, manager};

#[derive(Args, Debug, Clone)]
pub struct Install {
    /// Path to the `zim` binary the service should run.
    /// Defaults to the binary that handled this command
    /// (`std::env::current_exe()`).
    #[arg(long)]
    pub program: Option<PathBuf>,

    /// Don't autostart on login/boot. Default is to autostart.
    #[arg(long)]
    pub no_autostart: bool,
}

#[derive(Debug, serde::Serialize)]
pub struct InstallOutput {
    pub program: PathBuf,
    pub autostart: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("resolve zim binary path: {0}")]
    Program(std::io::Error),
    #[error("service manager: {0}")]
    Manager(std::io::Error),
    #[error("install: {0}")]
    Install(std::io::Error),
}

#[async_trait]
impl Op for Install {
    type Context = ();
    type Output = InstallOutput;
    type Error = InstallError;

    async fn build_context(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn run(&self, _ctx: ()) -> Result<Self::Output, Self::Error> {
        let program = match self.program.clone() {
            Some(p) => p,
            None => std::env::current_exe().map_err(InstallError::Program)?,
        };
        let autostart = !self.no_autostart;

        let mgr = manager().map_err(InstallError::Manager)?;
        mgr.install(ServiceInstallCtx {
            label: label(),
            program: program.clone(),
            args: vec![OsString::from("daemon"), OsString::from("run")],
            contents: None,
            username: None,
            working_directory: None,
            environment: None,
            autostart,
            disable_restart_on_failure: false,
        })
        .map_err(InstallError::Install)?;

        Ok(InstallOutput { program, autostart })
    }
}

impl fmt::Display for InstallOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let autostart_note = if self.autostart {
            ui::dim("autostart on login")
        } else {
            ui::dim("manual start")
        };
        write!(
            f,
            "{} ({}) — {}\n  next: zim daemon service start",
            ui::success("installed", &label().to_qualified_name()),
            ui::dim(self.program.display().to_string()),
            autostart_note,
        )
    }
}
