//! The OS service manager behind `zim daemon service`: launchd driven
//! directly on macOS (see [`launchd`]), the `service-manager` crate for
//! systemd and Windows.

use std::ffi::OsString;
use std::io;
use std::path::Path;

use service_manager::{
    ServiceInstallCtx, ServiceLevel, ServiceManager, ServiceStartCtx, ServiceStatus,
    ServiceStatusCtx, ServiceStopCtx, ServiceUninstallCtx,
};

use super::{label, launchd};

/// Arguments the service runs `zim` with.
const DAEMON_ARGS: [&str; 2] = ["daemon", "run"];

/// Register the daemon, replacing any previous registration. With
/// `autostart` it runs now and on every login.
pub fn install(program: &Path, autostart: bool) -> io::Result<()> {
    if launchd::active() {
        // `RunAtLoad` starts an autostart job as it loads.
        return launchd::install(&qualified_name(), program, &DAEMON_ARGS, autostart);
    }
    let mgr = manager()?;
    mgr.install(ServiceInstallCtx {
        label: label(),
        program: program.to_path_buf(),
        args: DAEMON_ARGS.iter().map(OsString::from).collect(),
        contents: None,
        username: None,
        working_directory: None,
        environment: None,
        autostart,
        disable_restart_on_failure: false,
    })?;
    // Other managers only enable an autostart unit for next boot, so
    // start it now — autostart means running, on every platform.
    if autostart {
        mgr.start(ServiceStartCtx { label: label() })?;
    }
    Ok(())
}

pub fn uninstall() -> io::Result<()> {
    if launchd::active() {
        return launchd::uninstall(&qualified_name());
    }
    manager()?.uninstall(ServiceUninstallCtx { label: label() })
}

pub fn start() -> io::Result<()> {
    if launchd::active() {
        return launchd::start(&qualified_name());
    }
    manager()?.start(ServiceStartCtx { label: label() })
}

/// Ask the daemon to stop. Fire-and-forget: returns before it drains.
pub fn stop() -> io::Result<()> {
    if launchd::active() {
        return launchd::stop(&qualified_name());
    }
    manager()?.stop(ServiceStopCtx { label: label() })
}

pub fn status() -> io::Result<ServiceStatus> {
    if launchd::active() {
        return launchd::status(&qualified_name());
    }
    manager()?.status(ServiceStatusCtx { label: label() })
}

fn qualified_name() -> String {
    label().to_qualified_name()
}

/// Native, user-level service manager for the current OS.
fn manager() -> io::Result<Box<dyn ServiceManager>> {
    let mut m = <dyn ServiceManager>::native()?;
    m.set_level(ServiceLevel::User)?;
    Ok(m)
}
