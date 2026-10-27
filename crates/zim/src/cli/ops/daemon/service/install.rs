//! `zim daemon service install` — register the daemon as a user service.

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use async_trait::async_trait;
use clap::Args;
use service_manager::{ServiceInstallCtx, ServiceStartCtx};

use crate::cli::op::Op;
use crate::cli::ui;

use super::{label, launchd, manager};

/// Arguments the service runs `zim` with.
const DAEMON_ARGS: [&str; 2] = ["daemon", "run"];

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
    /// Whether the daemon was started as part of installing.
    pub started: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("resolve zim binary path: {0}")]
    Program(std::io::Error),
    #[error("service manager: {0}")]
    Manager(std::io::Error),
    #[error("install: {0}")]
    Install(std::io::Error),
    #[error("start: {0}")]
    Start(std::io::Error),
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
        let qn = label().to_qualified_name();
        mgr.install(ServiceInstallCtx {
            label: label(),
            program: program.clone(),
            args: DAEMON_ARGS.iter().map(OsString::from).collect(),
            // service-manager's launchd plist keeps the daemon alive
            // through a clean stop; ours only restarts it on a crash.
            contents: launchd::active()
                .then(|| launchd::plist(&qn, &program, &DAEMON_ARGS, autostart)),
            username: None,
            working_directory: None,
            environment: None,
            autostart,
            disable_restart_on_failure: false,
        })
        .map_err(InstallError::Install)?;

        // launchd starts an autostart job at load (`RunAtLoad`); other
        // managers only enable it for next boot, so start it now —
        // autostart means running, on every platform.
        if autostart && !launchd::active() {
            mgr.start(ServiceStartCtx { label: label() })
                .map_err(InstallError::Start)?;
        }

        Ok(InstallOutput {
            program,
            autostart,
            started: autostart,
        })
    }
}

impl fmt::Display for InstallOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let qn = label().to_qualified_name();
        let program = ui::dim(self.program.display().to_string());
        if self.started {
            write!(
                f,
                "{} ({}) — {}",
                ui::success("installed and started", &qn),
                program,
                ui::dim("autostart on login"),
            )
        } else {
            write!(
                f,
                "{} ({}) — {}\n  next: zim daemon service start",
                ui::success("installed", &qn),
                program,
                ui::dim("manual start"),
            )
        }
    }
}
