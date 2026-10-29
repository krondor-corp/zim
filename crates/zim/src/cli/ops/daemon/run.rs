//! `zim daemon run` — runs the HTTP API server in the foreground.
//!
//! This is the daemon process itself; service-manager commands
//! (install/start/stop/...) just wrap it. Doesn't go through
//! `ApiClient`; boots a `ServiceState` and supervises the HTTP server
//! and peer via [`zim_peer::ShutdownHandle`] — the same idiom the hub
//! uses. A service that exits before the shutdown signal (e.g. the
//! peer's effect runner dying) trips a loud exit(2), so the service
//! manager restarts us instead of running half-dead.
//!
//! Before booting anything it takes the exclusive `$ZIM_HOME/daemon.lock`
//! (see [`crate::context::daemon_lock`]) so a home only ever has one
//! daemon, then records the actually-bound API port in it once the
//! listener is up.

use std::fmt;
use std::net::SocketAddr;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::context::daemon_lock::{self, LockError};
use crate::context::{ContextError, DaemonContext};
use crate::daemon::state::{ServiceState, StateError};
use crate::daemon::Config;

#[derive(Args, Debug, Clone)]
pub struct Run {
    /// Override the listen port (otherwise read from config.toml).
    #[arg(long)]
    pub port: Option<u16>,
    /// Bind address (loopback by default — daemon isn't exposed
    /// off-host).
    #[arg(long, default_value = "127.0.0.1")]
    pub bind: String,
}

#[derive(Debug, serde::Serialize)]
pub struct RunOutput {}

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Context(#[from] ContextError),
    #[error(transparent)]
    State(#[from] StateError),
    #[error(transparent)]
    Lock(#[from] LockError),
    #[error("listener bind {addr}: {source}")]
    Bind {
        addr: SocketAddr,
        source: std::io::Error,
    },
    #[error("invalid bind: {0}")]
    BadBind(String),
}

#[async_trait]
impl Op for Run {
    type Context = DaemonContext;
    type Output = RunOutput;
    type Error = RunError;

    async fn build_context(&self) -> Result<DaemonContext, Self::Error> {
        Ok(DaemonContext::build(None)?)
    }

    async fn run(&self, ctx: DaemonContext) -> Result<Self::Output, Self::Error> {
        let port = self.port.unwrap_or(ctx.config.api_port);
        let addr: SocketAddr = format!("{}:{}", self.bind, port)
            .parse()
            .map_err(|e: std::net::AddrParseError| RunError::BadBind(e.to_string()))?;

        // One daemon per home: refuse before touching any store. Held
        // until this fn returns (i.e. the process lifetime).
        let mut lock = daemon_lock::acquire(&ctx.home)?;

        let state = ServiceState::boot(&ctx.home).await?;

        // Re-mount any vaults registered with `--auto`. Best-effort: a missing
        // mountpoint or busy vault is logged, not fatal.
        #[cfg(feature = "fuse")]
        state.mounts().start_auto().await;

        let bind_err = |source| RunError::Bind { addr, source };
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(bind_err)?;
        // Record the real port (`--port 0` binds an ephemeral one).
        let bound = listener.local_addr().map_err(bind_err)?;
        lock.record(bound.port())?;

        let config = Config::new(bound);

        let (mut handle, shutdown_rx) = zim_peer::ShutdownHandle::new();
        let http_state = state.clone();
        let http_rx = shutdown_rx.clone();
        handle.push(
            "http",
            tokio::spawn(async move {
                if let Err(e) = crate::daemon::run_api(listener, config, http_state, http_rx).await
                {
                    tracing::error!("http server error: {e}");
                }
            }),
        );
        handle.push("peer", state.peer().spawn(shutdown_rx));

        handle.wait().await;

        // Spin mounts down with the daemon — a dead mount is worse than
        // no mount (Finder shows a ghost volume; IO hangs).
        #[cfg(feature = "fuse")]
        state.mounts().stop_all().await;

        drop(lock);
        Ok(RunOutput {})
    }
}

impl fmt::Display for RunOutput {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}
