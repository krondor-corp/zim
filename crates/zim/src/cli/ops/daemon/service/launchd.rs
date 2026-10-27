//! launchd specifics the `service-manager` crate gets wrong for us.
//!
//! service-manager renders `KeepAlive: true` into every plist it
//! writes. Under that policy launchd respawns the job no matter how it
//! exits, so `launchctl stop` is a no-op in practice: the daemon drains,
//! exits, and comes straight back. We write our own plist instead with
//! `KeepAlive: { SuccessfulExit: false }` — launchd restarts us only
//! when we crash (`zim daemon run` exits non-zero on a dead service)
//! and leaves us down after a clean SIGTERM drain.

use std::io;
use std::path::{Path, PathBuf};

use plist::{Dictionary, Value};

/// Seconds launchd waits between SIGTERM and SIGKILL. The daemon's
/// graceful drain takes several seconds; a SIGKILL mid-drain would
/// read as a crash and trigger a respawn.
const EXIT_TIMEOUT_SECS: i64 = 30;

/// Whether the OS service manager is launchd (macOS).
pub fn active() -> bool {
    cfg!(target_os = "macos")
}

/// Render the daemon's LaunchAgent plist.
pub fn plist(label: &str, program: &Path, args: &[&str], autostart: bool) -> String {
    let mut keep_alive = Dictionary::new();
    keep_alive.insert("SuccessfulExit".into(), Value::Boolean(false));

    let program_arguments = std::iter::once(program.to_string_lossy().into_owned())
        .chain(args.iter().map(|a| a.to_string()))
        .map(Value::String)
        .collect();

    let mut dict = Dictionary::new();
    dict.insert("Label".into(), Value::String(label.into()));
    dict.insert("ProgramArguments".into(), Value::Array(program_arguments));
    dict.insert("KeepAlive".into(), Value::Dictionary(keep_alive));
    dict.insert("RunAtLoad".into(), Value::Boolean(autostart));
    dict.insert(
        "ExitTimeOut".into(),
        Value::Integer(EXIT_TIMEOUT_SECS.into()),
    );

    let mut buf = Vec::new();
    Value::Dictionary(dict)
        .to_writer_xml(&mut buf)
        .expect("plist serializes to memory");
    String::from_utf8(buf).expect("plist XML is UTF-8")
}

/// Whether the installed plist would respawn the daemon after a clean
/// stop — i.e. it was written by an older `zim` with a bare
/// `KeepAlive: true`. `false` when not on launchd or not installed.
pub fn respawns_on_stop(label: &str) -> io::Result<bool> {
    if !active() {
        return Ok(false);
    }
    let path = plist_path(label)?;
    match std::fs::read(&path) {
        Ok(bytes) => Ok(keeps_alive_unconditionally(&bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

fn keeps_alive_unconditionally(plist_bytes: &[u8]) -> bool {
    let Ok(Value::Dictionary(dict)) = Value::from_reader(io::Cursor::new(plist_bytes)) else {
        return false;
    };
    matches!(dict.get("KeepAlive"), Some(Value::Boolean(true)))
}

fn plist_path(label: &str) -> io::Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home directory"))?;
    Ok(home
        .join("Library/LaunchAgents")
        .join(format!("{label}.plist")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alice_can_stop_a_daemon_installed_with_our_plist() {
        // Alice installs the service; our plist only respawns on crash.
        let xml = plist(
            "org.zim.daemon",
            Path::new("/usr/local/bin/zim"),
            &["daemon", "run"],
            true,
        );

        // A clean stop leaves the daemon down.
        assert!(!keeps_alive_unconditionally(xml.as_bytes()));

        // And the plist still runs `zim daemon run` at load.
        let Value::Dictionary(dict) = Value::from_reader(io::Cursor::new(xml.as_bytes())).unwrap()
        else {
            panic!("plist root is a dictionary");
        };
        assert_eq!(dict.get("RunAtLoad"), Some(&Value::Boolean(true)));
        let args: Vec<_> = dict
            .get("ProgramArguments")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .filter_map(Value::as_string)
            .collect();
        assert_eq!(args, ["/usr/local/bin/zim", "daemon", "run"]);
    }

    #[test]
    fn bobs_legacy_install_is_flagged_as_respawning() {
        // Bob installed with an older zim: service-manager's bare KeepAlive.
        let legacy = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>org.zim.daemon</string>
  <key>KeepAlive</key><true/>
  <key>RunAtLoad</key><true/>
</dict>
</plist>"#;

        // launchd would bring his daemon straight back after a stop.
        assert!(keeps_alive_unconditionally(legacy.as_bytes()));
    }
}
