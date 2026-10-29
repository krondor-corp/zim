//! `$ZIM_HOME/daemon.lock` — one daemon per home, and how others find it.
//!
//! `zim daemon run` takes an exclusive OS advisory lock (`flock` on
//! Unix, `LockFileEx` on Windows, via std's `File::try_lock`) on the
//! lock file before opening any store, and holds it for the whole
//! process lifetime. Once the HTTP listener is bound it writes a small
//! JSON record (`pid`, `api_port`, `started_at`) into the same file.
//!
//! Readers ([`probe`]) decide "running" by *attempting* a shared lock,
//! never by trusting the file contents: the OS drops the lock when the
//! daemon dies, so a stale record left behind by a crash reads as "not
//! running". The record is only consulted once the lock says someone
//! holds the home.
//!
//! Windows locks are mandatory byte-range locks, so a reader there
//! can't read the record while the daemon holds it — [`probe`] still
//! reports the daemon as running, just without pid/port.

use std::fmt;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::paths;

/// How many times [`acquire`] retries a held lock before giving up. A
/// concurrent [`probe`] holds a shared lock for microseconds; retrying
/// keeps a CLI probe from spuriously refusing a booting daemon.
const ACQUIRE_ATTEMPTS: u32 = 5;
const ACQUIRE_BACKOFF: Duration = Duration::from_millis(20);

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("a daemon is already running for {} ({running})", home.display())]
    AlreadyRunning {
        home: PathBuf,
        running: RunningDaemon,
    },
    #[error("daemon lock {}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// What a live daemon wrote into its lock file. Fields are `None` when
/// the record couldn't be read (still booting, mid-write, or Windows).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RunningDaemon {
    pub pid: Option<u32>,
    pub api_port: Option<u16>,
}

/// On-disk JSON shape of the lock file.
#[derive(Debug, Serialize, Deserialize)]
struct LockRecord {
    pid: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    api_port: Option<u16>,
    started_at: DateTime<Utc>,
}

/// The held lock. Dropping it (or the process exiting) releases the home.
#[derive(Debug)]
pub struct DaemonLock {
    file: File,
    path: PathBuf,
    started_at: DateTime<Utc>,
}

/// Take the exclusive lock on `home`'s daemon lock file, without
/// blocking. Fails with [`LockError::AlreadyRunning`] if another
/// process holds it. On success the file records our pid (no port yet
/// — see [`DaemonLock::record`]).
pub fn acquire(home: &Path) -> Result<DaemonLock, LockError> {
    let path = paths::daemon_lock_file(home);
    let io = |source| LockError::Io {
        path: path.clone(),
        source,
    };
    // No truncate: the file may belong to a live daemon until we hold
    // the lock.
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(io)?;

    let mut attempt = 1;
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) if attempt < ACQUIRE_ATTEMPTS => {
                attempt += 1;
                std::thread::sleep(ACQUIRE_BACKOFF);
            }
            Err(TryLockError::WouldBlock) => {
                return Err(LockError::AlreadyRunning {
                    home: home.to_path_buf(),
                    running: read_record(&file),
                });
            }
            Err(TryLockError::Error(e)) => return Err(io(e)),
        }
    }

    let mut lock = DaemonLock {
        file,
        path,
        started_at: Utc::now(),
    };
    lock.write(None)?;
    Ok(lock)
}

/// Is a daemon currently holding `home`? `Some` only while another
/// process holds the lock; a missing or stale lock file is `None`.
/// This is what the CLI (and the desktop app) use to find the running
/// daemon's port.
pub fn probe(home: &Path) -> Option<RunningDaemon> {
    let file = File::open(paths::daemon_lock_file(home)).ok()?;
    match file.try_lock_shared() {
        // Nobody holds it — whatever the file says is stale. The
        // shared lock drops with `file`.
        Ok(()) => None,
        Err(TryLockError::WouldBlock) => Some(read_record(&file)),
        Err(TryLockError::Error(e)) => {
            tracing::debug!("daemon lock probe failed: {e}");
            None
        }
    }
}

/// Best-effort read of a held lock file's record.
fn read_record(mut file: &File) -> RunningDaemon {
    let mut body = String::new();
    if file.seek(SeekFrom::Start(0)).is_err() || file.read_to_string(&mut body).is_err() {
        return RunningDaemon::default();
    }
    match serde_json::from_str::<LockRecord>(&body) {
        Ok(r) => RunningDaemon {
            pid: Some(r.pid),
            api_port: r.api_port,
        },
        Err(_) => RunningDaemon::default(),
    }
}

impl DaemonLock {
    /// Record the port the API listener actually bound, so [`probe`]
    /// can hand it to clients.
    pub fn record(&mut self, api_port: u16) -> Result<(), LockError> {
        self.write(Some(api_port))
    }

    /// Rewrite the record through the locked handle.
    fn write(&mut self, api_port: Option<u16>) -> Result<(), LockError> {
        let record = LockRecord {
            pid: std::process::id(),
            api_port,
            started_at: self.started_at,
        };
        let body = serde_json::to_vec(&record).expect("lock record serializes");
        let mut f = &self.file;
        f.set_len(0)
            .and_then(|()| f.seek(SeekFrom::Start(0)).map(|_| ()))
            .and_then(|()| f.write_all(&body))
            .and_then(|()| f.flush())
            .map_err(|source| LockError::Io {
                path: self.path.clone(),
                source,
            })
    }
}

impl fmt::Display for RunningDaemon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.pid {
            Some(pid) => write!(f, "pid {pid}")?,
            None => write!(f, "pid unknown")?,
        }
        match self.api_port {
            Some(port) => write!(f, ", port {port}"),
            None => write!(f, ", port not yet bound"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn probe_finds_running_daemon_port() {
        // Alice's daemon takes her home and binds port 17200.
        let alice_home = tempfile::tempdir().unwrap();
        let mut alice_daemon = acquire(alice_home.path()).expect("alice takes her home");
        alice_daemon.record(17200).unwrap();

        // Her CLI probes the home and finds the daemon's pid and port.
        let running = probe(alice_home.path()).expect("daemon is running");
        assert_eq!(running.pid, Some(std::process::id()));
        assert_eq!(running.api_port, Some(17200));
    }

    #[test]
    fn probe_sees_booting_daemon_before_port_is_bound() {
        // Alice's daemon holds the home but hasn't bound its listener yet.
        let alice_home = tempfile::tempdir().unwrap();
        let _alice_daemon = acquire(alice_home.path()).unwrap();

        // It's running — just without a port to dial.
        let running = probe(alice_home.path()).expect("daemon is running");
        assert_eq!(running.pid, Some(std::process::id()));
        assert_eq!(running.api_port, None);
    }

    #[test]
    fn alice_second_daemon_is_refused_while_first_holds_home() {
        // Alice's first daemon is up on 17201.
        let alice_home = tempfile::tempdir().unwrap();
        let mut first = acquire(alice_home.path()).unwrap();
        first.record(17201).unwrap();

        // A second daemon on the same home is refused, naming the first.
        let err = acquire(alice_home.path()).expect_err("home is taken");
        match &err {
            LockError::AlreadyRunning { home, running } => {
                assert_eq!(home, alice_home.path());
                assert_eq!(running.api_port, Some(17201));
            }
            other => panic!("expected AlreadyRunning, got {other:?}"),
        }
        assert!(err.to_string().contains("port 17201"));

        // The refusal didn't clobber the first daemon's record.
        assert_eq!(probe(alice_home.path()).unwrap().api_port, Some(17201));
    }

    #[test]
    fn stale_lock_file_after_crash_is_not_running() {
        // Alice's daemon records its port, then dies without cleanup.
        let alice_home = tempfile::tempdir().unwrap();
        let mut alice_daemon = acquire(alice_home.path()).unwrap();
        alice_daemon.record(17202).unwrap();
        drop(alice_daemon);

        // The file (and its record) is still there…
        let lock_path = paths::daemon_lock_file(alice_home.path());
        assert!(std::fs::read_to_string(&lock_path)
            .unwrap()
            .contains("17202"));

        // …but nobody holds the lock, so nothing is running.
        assert_eq!(probe(alice_home.path()), None);

        // And a fresh daemon can take the home over.
        let mut restarted = acquire(alice_home.path()).expect("stale lock is reclaimable");
        restarted.record(17203).unwrap();
        assert_eq!(probe(alice_home.path()).unwrap().api_port, Some(17203));
    }

    #[test]
    fn bob_fresh_home_has_no_daemon() {
        // Bob never started a daemon: no lock file at all.
        let bob_home = tempfile::tempdir().unwrap();
        assert_eq!(probe(bob_home.path()), None);
    }
}
