//! Connect-or-start: find the daemon holding `$ZIM_HOME`, or launch one.
//!
//! The decision logic is generic over [`DaemonHost`] so it tests without
//! touching a real lock file, process, or socket. [`crate::sidecar`]
//! supplies the real host.
//!
//! - lock held with a port → wait for `/_status/livez`, connect.
//! - lock held, port not bound yet (daemon booting) → wait for the port.
//! - nobody holds the home → spawn the daemon, then wait the same way.
//!
//! A spawn that loses a race to another launcher is harmless: the daemon
//! lock admits one, and polling then finds the winner.

use std::time::Duration;

/// What the desktop app needs to know about the daemon it talks to.
pub trait DaemonHost {
    /// The API port of the daemon currently holding the home.
    /// `None` = not running; `Some(None)` = running, port not bound yet.
    fn probe(&self) -> Option<Option<u16>>;
    /// Launch a daemon that outlives this app.
    fn spawn(&self) -> std::io::Result<()>;
    /// Does `port` answer `/_status/livez`?
    async fn healthy(&self, port: u16) -> bool;
    /// If the daemon we spawned has already died, why. A spawn that lost
    /// a race to another daemon also exits — callers only trust this
    /// while [`probe`](Self::probe) still finds nobody.
    fn spawned_exit(&self) -> Option<String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connection {
    /// A daemon was already running (the CLI's, or an earlier app run's).
    Existing { port: u16 },
    /// We launched it.
    Started { port: u16 },
}

impl Connection {
    pub fn port(&self) -> u16 {
        match self {
            Connection::Existing { port } | Connection::Started { port } => *port,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("could not start the zim daemon: {0}")]
    Spawn(std::io::Error),
    #[error(
        "the zim daemon exited right after starting ({0}); see the daemon log in $ZIM_HOME/state"
    )]
    Exited(String),
    #[error("the zim daemon did not become healthy within {0:?} (see the daemon log in $ZIM_HOME/state)")]
    Timeout(Duration),
}

/// How long to wait, and how often to look.
#[derive(Debug, Clone, Copy)]
pub struct Patience {
    pub timeout: Duration,
    pub poll: Duration,
}

impl Default for Patience {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            poll: Duration::from_millis(150),
        }
    }
}

pub async fn connect_or_start<H: DaemonHost>(
    host: &H,
    patience: Patience,
) -> Result<Connection, ConnectError> {
    let spawned = match host.probe() {
        Some(_) => false,
        None => {
            host.spawn().map_err(ConnectError::Spawn)?;
            true
        }
    };

    let wait = async {
        loop {
            match host.probe() {
                Some(Some(port)) if host.healthy(port).await => return Ok(port),
                None if spawned => {
                    if let Some(why) = host.spawned_exit() {
                        return Err(ConnectError::Exited(why));
                    }
                }
                _ => {}
            }
            tokio::time::sleep(patience.poll).await;
        }
    };
    let port = tokio::time::timeout(patience.timeout, wait)
        .await
        .map_err(|_| ConnectError::Timeout(patience.timeout))??;

    Ok(if spawned {
        Connection::Started { port }
    } else {
        Connection::Existing { port }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::io;

    const FAST: Patience = Patience {
        timeout: Duration::from_millis(300),
        poll: Duration::from_millis(5),
    };

    /// A scripted daemon: what each probe sees, whether it is healthy,
    /// and whether spawning works.
    struct Fake {
        probes: RefCell<Vec<Option<Option<u16>>>>,
        healthy: bool,
        spawn_ok: bool,
        spawned: Cell<u32>,
        /// Set once the spawned daemon "dies".
        exit: Option<String>,
    }

    impl Fake {
        fn new(probes: Vec<Option<Option<u16>>>, healthy: bool, spawn_ok: bool) -> Self {
            Self {
                probes: RefCell::new(probes),
                healthy,
                spawn_ok,
                spawned: Cell::new(0),
                exit: None,
            }
        }
        fn dying(mut self, why: &str) -> Self {
            self.exit = Some(why.to_string());
            self
        }
    }

    impl DaemonHost for Fake {
        fn probe(&self) -> Option<Option<u16>> {
            let mut p = self.probes.borrow_mut();
            if p.len() > 1 {
                p.remove(0)
            } else {
                p[0]
            }
        }
        fn spawn(&self) -> io::Result<()> {
            self.spawned.set(self.spawned.get() + 1);
            if self.spawn_ok {
                Ok(())
            } else {
                Err(io::Error::new(io::ErrorKind::NotFound, "no zim binary"))
            }
        }
        async fn healthy(&self, _port: u16) -> bool {
            self.healthy
        }
        fn spawned_exit(&self) -> Option<String> {
            self.exit.clone()
        }
    }

    #[tokio::test]
    async fn alice_opens_the_app_while_her_cli_daemon_is_running() {
        // Alice's daemon already holds her home on 17200 and is healthy.
        let host = Fake::new(vec![Some(Some(17200))], true, true);

        let conn = connect_or_start(&host, FAST).await.unwrap();

        // The app connects to it and never spawns a second one.
        assert_eq!(conn, Connection::Existing { port: 17200 });
        assert_eq!(host.spawned.get(), 0);
    }

    #[tokio::test]
    async fn alice_opens_the_app_with_no_daemon_and_it_starts_one() {
        // Nobody holds the home; after the spawn the daemon takes it,
        // first without a port, then bound on 17201.
        let host = Fake::new(vec![None, Some(None), Some(Some(17201))], true, true);

        let conn = connect_or_start(&host, FAST).await.unwrap();

        assert_eq!(conn, Connection::Started { port: 17201 });
        assert_eq!(host.spawned.get(), 1);
    }

    #[tokio::test]
    async fn alice_opens_the_app_while_a_daemon_is_still_booting() {
        // Her daemon holds the home but hasn't bound its port yet.
        let host = Fake::new(vec![Some(None), Some(Some(17202))], true, true);

        let conn = connect_or_start(&host, FAST).await.unwrap();

        // Waits for the port rather than spawning over it.
        assert_eq!(conn, Connection::Existing { port: 17202 });
        assert_eq!(host.spawned.get(), 0);
    }

    #[tokio::test]
    async fn bob_gets_a_timeout_when_the_daemon_never_turns_healthy() {
        // A daemon holds the port but never answers livez.
        let host = Fake::new(vec![None, Some(Some(17203))], false, true);

        let err = connect_or_start(&host, FAST).await.unwrap_err();

        assert!(matches!(err, ConnectError::Timeout(_)), "got {err:?}");
        assert_eq!(host.spawned.get(), 1);
    }

    #[tokio::test]
    async fn bob_is_told_when_the_zim_binary_cannot_be_launched() {
        // Nothing runs and the sidecar is missing.
        let host = Fake::new(vec![None], true, false);

        let err = connect_or_start(&host, FAST).await.unwrap_err();

        assert!(matches!(err, ConnectError::Spawn(_)), "got {err:?}");
        assert!(err.to_string().contains("no zim binary"));
    }

    #[tokio::test]
    async fn carol_hears_why_the_daemon_died_instead_of_waiting_out_the_clock() {
        // Her daemon can't bind its port: it exits and never takes the home.
        let host = Fake::new(vec![None], true, true).dying("exit status 1");

        let err = connect_or_start(&host, FAST).await.unwrap_err();

        assert!(matches!(err, ConnectError::Exited(_)), "got {err:?}");
        assert!(err.to_string().contains("exit status 1"));
    }
}
