//! Service lifecycle + graceful shutdown.
//!
//! Deliberately zim-free (only tokio/futures/tracing) — aesthetic adopted
//! from `krondor-corp/pack`'s `runtime` module, and a candidate for
//! extraction into a shared crate maintained outside this repo. Keep it
//! that way: no zim types in here. Every
//! long-running component (the HTTP server, SSE pumps, task workers, the
//! daemon's iroh router, sync provider) implements [`Service`];
//! [`ShutdownHandle`] listens for SIGINT/SIGTERM, broadcasts a single shutdown
//! signal, and waits for all services to drain.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::watch;
use tokio::task::JoinHandle;

const FINAL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);
const REQUEST_GRACE_PERIOD: Duration = Duration::from_secs(10);

#[async_trait::async_trait]
pub trait Service: Send + 'static {
    type State: Clone + Send + Sync + 'static;

    async fn run(state: Self::State, shutdown_rx: watch::Receiver<()>);

    fn spawn(state: Self::State, shutdown_rx: watch::Receiver<()>) -> JoinHandle<()> {
        tokio::spawn(Self::run(state, shutdown_rx))
    }
}

pub struct ShutdownHandle {
    graceful_waiter: JoinHandle<()>,
    shutdown_started: Arc<AtomicBool>,
    handles: Vec<(&'static str, JoinHandle<()>)>,
}

impl ShutdownHandle {
    pub fn new() -> (Self, watch::Receiver<()>) {
        let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT handler");
        let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");

        let (tx, rx) = watch::channel(());
        let shutdown_started = Arc::new(AtomicBool::new(false));
        let started_flag = shutdown_started.clone();

        let graceful_waiter = tokio::spawn(async move {
            tokio::select! {
                _ = sigint.recv() => {
                    tracing::debug!("graceful exit on SIGINT");
                }
                _ = sigterm.recv() => {
                    tokio::time::sleep(REQUEST_GRACE_PERIOD).await;
                    tracing::debug!("graceful shutdown with delay on SIGTERM");
                }
            }
            started_flag.store(true, Ordering::SeqCst);
            let _ = tx.send(());
        });

        (
            Self {
                graceful_waiter,
                shutdown_started,
                handles: Vec::new(),
            },
            rx,
        )
    }

    pub fn push(&mut self, name: &'static str, handle: JoinHandle<()>) {
        self.handles.push((name, handle));
    }

    pub async fn wait(self) {
        let Self {
            graceful_waiter,
            shutdown_started,
            handles,
        } = self;

        let mut services: futures::stream::FuturesUnordered<_> = handles
            .into_iter()
            .map(|(name, h)| async move { (name, h.await) })
            .collect();

        let graceful = async move {
            let _ = graceful_waiter.await;
        };
        tokio::pin!(graceful);

        loop {
            tokio::select! {
                _ = &mut graceful => {
                    tracing::info!("shutdown signal received, draining services");
                    break;
                }
                Some((name, res)) = futures::StreamExt::next(&mut services) => {
                    let drained = shutdown_started.load(Ordering::SeqCst);
                    match &res {
                        Ok(()) if drained => {
                            tracing::debug!(service = name, "service drained");
                        }
                        Ok(()) => tracing::error!(
                            service = name,
                            "service exited before shutdown signal — exiting with code 2",
                        ),
                        Err(e) if e.is_panic() => tracing::error!(
                            service = name,
                            "service panicked{}: {e}",
                            if drained { " during drain" } else { " before shutdown signal" },
                        ),
                        Err(e) => tracing::error!(
                            service = name,
                            "service join error{}: {e}",
                            if drained { " during drain" } else { " before shutdown signal" },
                        ),
                    }
                    if !drained {
                        std::process::exit(2);
                    }
                }
            }
        }

        let remaining = futures::StreamExt::collect::<Vec<(&'static str, _)>>(services);
        if tokio::time::timeout(FINAL_SHUTDOWN_TIMEOUT, remaining)
            .await
            .is_err()
        {
            tracing::error!(
                "failed to shut down within {}s — force exiting",
                FINAL_SHUTDOWN_TIMEOUT.as_secs()
            );
            std::process::exit(4);
        }
        tracing::info!("shutdown complete");
    }
}
