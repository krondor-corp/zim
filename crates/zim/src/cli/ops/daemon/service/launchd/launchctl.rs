//! `launchctl`'s domain-target verbs (`bootstrap`, `bootout`,
//! `kickstart`, `kill`, `print`) against the user's `gui/<uid>` domain.
//!
//! We never touch the legacy `start`/`stop`/`load`/`remove` interface:
//! Apple's legacy launchctl has been seen to crash (SIGSEGV, exit 139)
//! on a wedged service record. Every failure carries launchctl's exit
//! status and output so a report says what launchd actually refused.

use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// launchctl's exit status for "no such service in this domain".
const NOT_FOUND: i32 = 113;

/// `bootout`'s exit status (EINPROGRESS) when the job is still exiting.
const IN_PROGRESS: i32 = 36;

/// Handle on the logged-in user's launchd domain.
pub struct Launchctl {
    /// `gui/<uid>`.
    domain: String,
}

/// What a `bootout` left behind.
pub enum Bootout {
    /// The service wasn't loaded to begin with.
    NotLoaded,
    /// launchd accepted the bootout; the record may linger until the
    /// job finishes draining.
    Requested,
}

impl Launchctl {
    /// The current user's GUI domain.
    pub fn user() -> io::Result<Self> {
        let out = Command::new("id").arg("-u").output()?;
        if !out.status.success() {
            return Err(failure("id -u", &out));
        }
        let uid = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(Self {
            domain: format!("gui/{uid}"),
        })
    }

    fn target(&self, label: &str) -> String {
        format!("{}/{label}", self.domain)
    }

    /// Load the job from `plist` into the domain. Starts it if the
    /// plist sets `RunAtLoad`.
    pub fn bootstrap(&self, plist: &Path) -> io::Result<()> {
        let plist = plist.to_string_lossy();
        run(&["bootstrap", &self.domain, &plist]).map(drop)
    }

    /// Unload the job, stopping it if it's running.
    pub fn bootout(&self, label: &str) -> io::Result<Bootout> {
        let target = self.target(label);
        let args = ["bootout", target.as_str()];
        let out = launchctl(&args)?;
        match out.status.code() {
            Some(0) | Some(IN_PROGRESS) => Ok(Bootout::Requested),
            Some(NOT_FOUND) => Ok(Bootout::NotLoaded),
            _ => Err(failure(&command_line(&args), &out)),
        }
    }

    /// Start the loaded job; with `kill`, kill a running instance
    /// first and start a fresh one.
    pub fn kickstart(&self, label: &str, kill: bool) -> io::Result<()> {
        let target = self.target(label);
        let args: &[&str] = if kill {
            &["kickstart", "-k", &target]
        } else {
            &["kickstart", &target]
        };
        run(args).map(drop)
    }

    /// Send `signal` (e.g. `SIGTERM`) to the running job.
    pub fn kill(&self, signal: &str, label: &str) -> io::Result<()> {
        run(&["kill", signal, &self.target(label)]).map(drop)
    }

    /// `launchctl print` of the job, or `None` when it isn't loaded.
    pub fn print(&self, label: &str) -> io::Result<Option<String>> {
        let target = self.target(label);
        let args = ["print", target.as_str()];
        let out = launchctl(&args)?;
        match out.status.code() {
            Some(0) => Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned())),
            Some(NOT_FOUND) => Ok(None),
            _ => Err(failure(&command_line(&args), &out)),
        }
    }
}

/// Run launchctl; any non-zero exit is an error.
fn run(args: &[&str]) -> io::Result<Output> {
    let out = launchctl(args)?;
    if out.status.success() {
        Ok(out)
    } else {
        Err(failure(&command_line(args), &out))
    }
}

fn launchctl(args: &[&str]) -> io::Result<Output> {
    Command::new("launchctl")
        .args(args)
        .output()
        .map_err(|e| io::Error::new(e.kind(), format!("run {}: {e}", command_line(args))))
}

fn command_line(args: &[&str]) -> String {
    format!("launchctl {}", args.join(" "))
}

/// `<command> failed (<exit status>): <stderr, or stdout if empty>`.
fn failure(command: &str, out: &Output) -> io::Error {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let detail = match (stderr.trim(), stdout.trim()) {
        ("", "") => "no output".to_string(),
        ("", s) | (s, _) => s.split_whitespace().collect::<Vec<_>>().join(" "),
    };
    io::Error::other(format!("{command} failed ({}): {detail}", out.status))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn output(code: i32, stdout: &str, stderr: &str) -> Output {
        use std::os::unix::process::ExitStatusExt;
        Output {
            // Wait-status encoding: exit code in the high byte.
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    #[cfg(unix)]
    #[test]
    fn alice_sees_why_launchd_refused_her_bootstrap() {
        // Alice's bootstrap fails; launchctl explains on stderr.
        let out = output(
            5,
            "",
            "Bootstrap failed: 5: Input/output error\nTry re-running the command as root\n",
        );

        let err = failure("launchctl bootstrap gui/501 /x.plist", &out).to_string();

        // The error names the command, the exit status, and the reason.
        assert!(err.starts_with("launchctl bootstrap gui/501 /x.plist failed"));
        assert!(err.contains("exit status: 5"), "{err}");
        assert!(err.contains("Bootstrap failed: 5: Input/output error"));
    }

    #[cfg(unix)]
    #[test]
    fn bob_gets_stdout_when_launchctl_says_nothing_on_stderr() {
        // Bob's launchctl writes its complaint to stdout instead.
        let out = output(3, "No such process\n", "");

        let err = failure("launchctl kill SIGTERM gui/501/org.zim.daemon", &out).to_string();

        assert!(err.contains("exit status: 3"), "{err}");
        assert!(err.ends_with("No such process"), "{err}");
    }
}
