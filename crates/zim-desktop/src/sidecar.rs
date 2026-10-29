//! The real [`DaemonHost`]: probe `daemon.lock`, launch the bundled `zim`
//! binary as a detached process, check `/_status/livez`.
//!
//! The daemon is a separate process on purpose: it keeps running after
//! the app quits, so the CLI and FUSE mounts keep working. That is why
//! it's spawned with `std::process` rather than Tauri's shell plugin,
//! which ties children to the app's lifetime.

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use url::Url;
use zim::context::{daemon_lock, paths};
use zim::daemon::api::client::ApiClient;
use zim::daemon::health::liveness::LivezRequest;

use crate::connect::DaemonHost;

const LIVEZ_TIMEOUT: Duration = Duration::from_millis(750);

pub struct SidecarHost {
    pub home: PathBuf,
    pub binary: PathBuf,
    child: Mutex<Option<Child>>,
}

impl SidecarHost {
    pub fn new(home: PathBuf, binary: PathBuf) -> Self {
        Self {
            home,
            binary,
            child: Mutex::new(None),
        }
    }
}

impl DaemonHost for SidecarHost {
    fn probe(&self) -> Option<Option<u16>> {
        daemon_lock::probe(&self.home).map(|running| running.api_port)
    }

    fn spawn(&self) -> std::io::Result<()> {
        let log_path = paths::daemon_log_path(&self.home);
        if let Some(dir) = log_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;
        let mut cmd = Command::new(&self.binary);
        // The same home the app resolved, so app + CLI share one identity.
        cmd.args(["daemon", "run"])
            .env("ZIM_HOME", &self.home)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        detach(&mut cmd);
        *self.child.lock().unwrap() = Some(cmd.spawn()?);
        Ok(())
    }

    fn spawned_exit(&self) -> Option<String> {
        let mut child = self.child.lock().unwrap();
        child
            .as_mut()?
            .try_wait()
            .ok()
            .flatten()
            .map(|status| status.to_string())
    }

    async fn healthy(&self, port: u16) -> bool {
        let Ok(base) = Url::parse(&format!("http://127.0.0.1:{port}")) else {
            return false;
        };
        let Ok(client) = ApiClient::new(&base) else {
            return false;
        };
        matches!(
            tokio::time::timeout(LIVEZ_TIMEOUT, client.call(LivezRequest {})).await,
            Ok(Ok(_))
        )
    }
}

/// Own session/process group so quitting the app doesn't signal the daemon.
#[cfg(unix)]
fn detach(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
}

#[cfg(windows)]
fn detach(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

/// Where the `zim` binary lives. First hit wins:
/// 1. `$ZIM_BIN` (explicit override),
/// 2. next to the app executable (a bundled `externalBin` lands there
///    with the target triple stripped),
/// 3. `binaries/zim-<triple>` in the source tree (`cargo tauri dev`),
/// 4. `zim` on `$PATH`.
pub fn resolve_binary(
    env_override: Option<OsString>,
    exe_dir: Option<&Path>,
    dev_dir: &Path,
    triple: &str,
) -> PathBuf {
    if let Some(p) = env_override.filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    let name = if cfg!(windows) { "zim.exe" } else { "zim" };
    let candidates = exe_dir
        .map(|d| d.join(name))
        .into_iter()
        .chain([dev_dir.join(format!("zim-{triple}{}", std::env::consts::EXE_SUFFIX))]);
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("zim"))
}

pub fn default_binary() -> PathBuf {
    let exe = std::env::current_exe().ok();
    resolve_binary(
        std::env::var_os("ZIM_BIN"),
        exe.as_deref().and_then(Path::parent),
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries"),
        env!("ZIM_DESKTOP_TARGET"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alice_pins_the_binary_with_zim_bin() {
        let dir = tempfile::tempdir().unwrap();
        let got = resolve_binary(Some("/opt/zim".into()), Some(dir.path()), dir.path(), "t");
        assert_eq!(got, PathBuf::from("/opt/zim"));
    }

    #[test]
    fn bob_gets_the_bundled_binary_next_to_the_app() {
        let app = tempfile::tempdir().unwrap();
        let bundled = app
            .path()
            .join(if cfg!(windows) { "zim.exe" } else { "zim" });
        std::fs::write(&bundled, b"").unwrap();

        let got = resolve_binary(None, Some(app.path()), Path::new("/nope"), "t");

        assert_eq!(got, bundled);
    }

    #[test]
    fn carol_in_dev_gets_the_triple_named_binary_from_the_source_tree() {
        let dev = tempfile::tempdir().unwrap();
        let built = dev.path().join(format!(
            "zim-aarch64-apple-darwin{}",
            std::env::consts::EXE_SUFFIX
        ));
        std::fs::write(&built, b"").unwrap();

        let got = resolve_binary(None, None, dev.path(), "aarch64-apple-darwin");

        assert_eq!(got, built);
    }

    #[test]
    fn dave_without_any_binary_falls_back_to_path() {
        let empty = tempfile::tempdir().unwrap();
        let got = resolve_binary(None, Some(empty.path()), empty.path(), "t");
        assert_eq!(got, PathBuf::from("zim"));
    }
}
