//! launchd (macOS), driven directly with launchctl's domain-target
//! verbs rather than through the `service-manager` crate, which still
//! shells out to the crash-prone legacy `start`/`stop`/`load`/`remove`.
//!
//! | op        | launchctl                                    |
//! |-----------|----------------------------------------------|
//! | install   | `bootout` (wait until gone) + `bootstrap`    |
//! | uninstall | `bootout` (wait until gone) + delete plist   |
//! | start     | `kickstart` (after `bootstrap` if unloaded)  |
//! | restart   | `kickstart -k`                               |
//! | stop      | `kill SIGTERM` (plist won't respawn on a clean exit) |
//! | status    | `print`                                      |

use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use service_manager::ServiceStatus;

mod launchctl;
mod plist;

use launchctl::{Bootout, Launchctl};
pub use plist::respawns_on_stop;

/// How long a booted-out job may take to leave the domain: its full
/// SIGTERM→SIGKILL window plus some slack.
const BOOTOUT_TIMEOUT: Duration = Duration::from_secs(plist::EXIT_TIMEOUT_SECS as u64 + 5);

/// Pause between `print` polls while waiting for a bootout.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Whether the OS service manager is launchd (macOS).
pub fn active() -> bool {
    cfg!(target_os = "macos")
}

/// Write the plist and load it, replacing any previous install. The
/// old job is fully gone before the new one loads, so a re-install
/// never runs two daemons racing for the API port.
pub fn install(label: &str, program: &Path, args: &[&str], autostart: bool) -> io::Result<()> {
    let ctl = Launchctl::user()?;
    unload(&ctl, label)?;

    let path = plist::plist_path(label)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, plist::plist(label, program, args, autostart))?;
    ctl.bootstrap(&path)
}

/// Unload the job and delete its plist. Not installed is not an error.
pub fn uninstall(label: &str) -> io::Result<()> {
    let ctl = Launchctl::user()?;
    unload(&ctl, label)?;
    match std::fs::remove_file(plist::plist_path(label)?) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Start the job; a no-op if it's already running.
pub fn start(label: &str) -> io::Result<()> {
    let ctl = Launchctl::user()?;
    ensure_loaded(&ctl, label)?;
    ctl.kickstart(label, false)
}

/// Kill any running instance and start a fresh one. launchd keeps one
/// instance per job, so the new daemon only starts once the old one
/// has exited.
pub fn restart(label: &str) -> io::Result<()> {
    let ctl = Launchctl::user()?;
    ensure_loaded(&ctl, label)?;
    ctl.kickstart(label, true)
}

/// SIGTERM the running job. Returns once the signal is sent, not once
/// the daemon has drained.
pub fn stop(label: &str) -> io::Result<()> {
    let ctl = Launchctl::user()?;
    let running = ctl
        .print(label)?
        .is_some_and(|p| matches!(parse_status(&p), ServiceStatus::Running));
    if running {
        ctl.kill("SIGTERM", label)?;
    }
    Ok(())
}

pub fn status(label: &str) -> io::Result<ServiceStatus> {
    let ctl = Launchctl::user()?;
    match ctl.print(label)? {
        Some(p) => Ok(parse_status(&p)),
        None if plist::plist_path(label)?.exists() => {
            Ok(ServiceStatus::Stopped(Some("not loaded".into())))
        }
        None => Ok(ServiceStatus::NotInstalled),
    }
}

/// Bootstrap the installed plist if the job isn't loaded.
fn ensure_loaded(ctl: &Launchctl, label: &str) -> io::Result<()> {
    if ctl.print(label)?.is_some() {
        return Ok(());
    }
    let path = plist::plist_path(label)?;
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "not installed — run `zim daemon service install`",
        ));
    }
    ctl.bootstrap(&path)
}

/// Boot the job out and wait until launchd has dropped it — which only
/// happens once its process has exited.
fn unload(ctl: &Launchctl, label: &str) -> io::Result<()> {
    if let Bootout::NotLoaded = ctl.bootout(label)? {
        return Ok(());
    }
    let deadline = Instant::now() + BOOTOUT_TIMEOUT;
    while ctl.print(label)?.is_some() {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "{label} still loaded {}s after bootout",
                    BOOTOUT_TIMEOUT.as_secs()
                ),
            ));
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    Ok(())
}

/// Read the job's state out of `launchctl print`. Only top-level
/// (single-tab) keys count; nested blocks have `state` keys of their own.
fn parse_status(print: &str) -> ServiceStatus {
    let field = |key: &str| {
        print.lines().find_map(|line| {
            let line = line.strip_prefix('\t')?;
            line.strip_prefix(key)?.strip_prefix(" = ").map(str::trim)
        })
    };
    if field("state") == Some("running") {
        return ServiceStatus::Running;
    }
    let reason = field("last exit code")
        .filter(|code| !code.starts_with('('))
        .map(|code| format!("last exit code {code}"));
    ServiceStatus::Stopped(reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alice_running_daemon_reads_as_running() {
        // Alice's daemon is up; a nested endpoint block has its own state.
        let print = "org.zim.daemon = {\n\
            \tactive count = 1\n\
            \tpath = /Users/alice/Library/LaunchAgents/org.zim.daemon.plist\n\
            \tstate = running\n\
            \tpid = 4242\n\
            \tendpoints = {\n\
            \t\tstate = inactive\n\
            \t}\n\
            \tlast exit code = (never exited)\n\
            }\n";

        assert!(matches!(parse_status(print), ServiceStatus::Running));
    }

    #[test]
    fn bobs_stopped_daemon_reports_its_last_exit() {
        // Bob stopped his daemon; only the nested block claims "running".
        let print = "org.zim.daemon = {\n\
            \tstate = not running\n\
            \tendpoints = {\n\
            \t\tstate = running\n\
            \t}\n\
            \tlast exit code = 0\n\
            }\n";

        match parse_status(print) {
            ServiceStatus::Stopped(reason) => {
                assert_eq!(reason.as_deref(), Some("last exit code 0"))
            }
            other => panic!("expected stopped, got {other:?}"),
        }
    }

    #[test]
    fn carols_never_started_daemon_has_no_exit_reason() {
        // Carol installed without autostart and never started it.
        let print = "org.zim.daemon = {\n\
            \tstate = not running\n\
            \tlast exit code = (never exited)\n\
            }\n";

        assert!(matches!(parse_status(print), ServiceStatus::Stopped(None)));
    }
}
