//! `zim daemon run` — runs the HTTP API server in the foreground.
//!
//! This is the daemon process itself; service-manager commands
//! (install/start/stop/...) just wrap it. Doesn't go through
//! `ApiClient`; boots a `ServiceState`, builds the axum router, and
//! serves until SIGINT/SIGTERM.

use std::fmt;
use std::net::SocketAddr;

use async_trait::async_trait;
use clap::Args;

use crate::cli::op::Op;
use crate::context::{ContextError, DaemonContext};
use crate::http_server::{self, Config};
use crate::service_state::{ServiceState, StateError};

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
    Server(#[from] http_server::HttpServerError),
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

        let state = ServiceState::boot(&ctx.home).await?;
        let config = Config::new(addr);

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(());
        let server_task = tokio::spawn(http_server::run_api(
            config,
            state.clone(),
            shutdown_rx.clone(),
        ));
        let peer = state.peer().clone();
        let peer_task = peer.spawn(shutdown_rx);

        wait_for_signal().await;
        let _ = shutdown_tx.send(());

        let _ = server_task.await;
        let _ = peer_task.await;

        Ok(RunOutput {})
    }
}

async fn wait_for_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT");
        let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM");
        tokio::select! {
            _ = sigint.recv() => {}
            _ = sigterm.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

impl fmt::Display for RunOutput {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Ok(())
    }
}
