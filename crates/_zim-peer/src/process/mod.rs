pub mod utils;

use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;

use tokio::sync::watch;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use zim_runtime::{Service, ShutdownHandle};

use crate::http_server;
use crate::{ServiceConfig, ServiceState};

/// Initialize logging, panic handler, and build info reporting.
/// Returns guards that must be kept alive for the duration of the program.
fn init_logging(
    service_config: &ServiceConfig,
) -> Vec<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::fmt::format::FmtSpan;

    let mut guards = Vec::new();

    // Stdout layer
    let (stdout_writer, stdout_guard) = tracing_appender::non_blocking(std::io::stdout());
    guards.push(stdout_guard);

    let stdout_env_filter = EnvFilter::builder()
        .with_default_directive(service_config.log_level.into())
        .from_env_lossy();

    let stdout_layer = tracing_subscriber::fmt::layer()
        .compact()
        .with_writer(stdout_writer)
        .with_filter(stdout_env_filter);

    // File layer (if log_dir is set)
    if let Some(log_dir) = &service_config.log_dir {
        // Create the log directory if it doesn't exist
        if let Err(e) = std::fs::create_dir_all(log_dir) {
            eprintln!(
                "Warning: Failed to create log directory {:?}: {}",
                log_dir, e
            );
        }

        let file_appender = tracing_appender::rolling::daily(log_dir, "zim.log");
        let (file_writer, file_guard) = tracing_appender::non_blocking(file_appender);
        guards.push(file_guard);

        let file_env_filter = EnvFilter::builder()
            .with_default_directive(service_config.log_level.into())
            .from_env_lossy();

        let file_layer = tracing_subscriber::fmt::layer()
            .with_writer(file_writer)
            .with_ansi(false)
            .with_span_events(FmtSpan::CLOSE)
            .with_filter(file_env_filter);

        tracing_subscriber::registry()
            .with(stdout_layer)
            .with(file_layer)
            .init();
    } else {
        tracing_subscriber::registry().with(stdout_layer).init();
    }

    utils::register_panic_logger();
    utils::report_build_info();

    guards
}

/// Create service state from config, exiting on error.
async fn create_state(
    service_config: &ServiceConfig,
) -> (ServiceState, crate::sync_provider::JobReceiver) {
    match ServiceState::from_config(service_config).await {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!("error creating server state: {}", e);
            std::process::exit(3);
        }
    }
}

/// Spawn the in-process peer services (iroh router + sync worker) into a
/// `ShutdownHandle`. Returns the state for callers that need to push more
/// services (HTTP, FUSE, etc.) before driving the handle to completion.
///
/// Used by both the standalone `zim-peer` daemon and the `zim-hub` gateway —
/// both embed the same peer-network and sync-worker tasks.
pub async fn spawn_peer_services(
    service_config: &ServiceConfig,
    handle: &mut ShutdownHandle,
    shutdown_rx: watch::Receiver<()>,
) -> ServiceState {
    let (state, job_receiver) = create_state(service_config).await;

    // Peer (iroh protocol router).
    let peer_for_spawn = state.peer().clone();
    let peer_shutdown = shutdown_rx.clone();
    handle.push(
        "peer",
        tokio::spawn(async move {
            if let Err(e) = zim_protocol::spawn(peer_for_spawn, peer_shutdown).await {
                tracing::error!("Peer error: {}", e);
            }
        }),
    );

    // Sync-provider worker — must take a shutdown_rx so it drains with the
    // rest of the daemon. Previously spawned bare inside ServiceState
    // and leaked past shutdown (T-007a-C).
    let sync_peer = state.peer().clone();
    let sync_stream = job_receiver.into_async();
    let sync_shutdown = shutdown_rx.clone();
    handle.push(
        "sync",
        tokio::spawn(async move {
            crate::sync_provider::run_worker(sync_peer, sync_stream, sync_shutdown).await;
        }),
    );

    // Backup sync service — polls active sync_targets and materializes
    // changed files to disk (T-018).
    let backup_state = crate::backup_sync::BackupSyncState {
        database: state.database().clone(),
        peer: state.peer().clone(),
    };
    handle.push(
        "backup-sync",
        crate::backup_sync::BackupSyncService::spawn(backup_state, shutdown_rx.clone()),
    );

    state
}

/// Create state and spawn background tasks, returning the state handle.
///
/// Use this when you need access to `ServiceState` (e.g. from Tauri IPC commands).
/// The returned `ShutdownHandle` must be kept alive; dropping it does not stop the service.
pub async fn start_service(service_config: &ServiceConfig) -> (ServiceState, ShutdownHandle) {
    let (mut handle, shutdown_rx) = ShutdownHandle::new();

    let state = spawn_peer_services(service_config, &mut handle, shutdown_rx.clone()).await;

    // API server.
    let api_port = service_config.api_port;
    let api_addr = SocketAddr::from_str(&format!("0.0.0.0:{}", api_port))
        .expect("Failed to parse API listen address");
    let api_state = state.clone();
    let api_config = http_server::Config::new(api_addr, service_config.gateway_url.clone());
    let api_rx = shutdown_rx.clone();
    handle.push(
        "api",
        tokio::spawn(async move {
            if let Err(e) = http_server::run_api(api_config, api_state, api_rx).await {
                tracing::error!("API server error: {}", e);
            }
        }),
    );

    // Gateway server.
    let gw_port = service_config.gateway_port;
    let gw_addr = SocketAddr::from_str(&format!("0.0.0.0:{}", gw_port))
        .expect("Failed to parse gateway listen address");
    let gw_state = state.clone();
    let gw_config = http_server::Config::new(gw_addr, service_config.gateway_url.clone());
    let gw_rx = shutdown_rx.clone();
    let gw_jax_dir = service_config.jax_dir.clone();
    handle.push(
        "gateway",
        tokio::spawn(async move {
            if let Err(e) = http_server::run_gateway(gw_config, gw_state, gw_jax_dir, gw_rx).await {
                tracing::error!("Gateway server error: {}", e);
            }
        }),
    );

    tracing::info!(
        "Running: Peer + API on port {} + Gateway on port {}",
        api_port,
        gw_port
    );

    // Start auto-mounts (with fuse feature). Spawn a task that fires the
    // auto-mount kick-off and another that stops mounts on shutdown.
    #[cfg(feature = "fuse")]
    {
        let mount_state = state.clone();
        tokio::spawn(async move {
            // Small delay to ensure services are ready
            tokio::time::sleep(Duration::from_millis(500)).await;

            let mount_manager = mount_state.mount_manager().read().await;
            if let Some(manager) = mount_manager.as_ref() {
                if let Err(e) = manager.start_auto().await {
                    tracing::error!("Failed to start auto-mounts: {}", e);
                }
            }
        });

        // FUSE mount drain on shutdown.
        let mount_drain_state = state.clone();
        let mut mount_drain_rx = shutdown_rx.clone();
        handle.push(
            "fuse-drain",
            tokio::spawn(async move {
                let _ = mount_drain_rx.changed().await;
                tracing::info!("Stopping all FUSE mounts...");
                let mount_manager = mount_drain_state.mount_manager().read().await;
                if let Some(manager) = mount_manager.as_ref() {
                    if let Err(e) = manager.stop_all().await {
                        tracing::error!("Failed to stop FUSE mounts: {}", e);
                    }
                }
            }),
        );
    }

    // Suppress unused warning when fuse feature is off.
    let _ = Duration::from_millis(0);

    (state, handle)
}

/// Spawns the daemon service: P2P peer + API server + gateway server.
/// Blocks until shutdown signal is received. Use for CLI binary usage.
pub async fn spawn_service(service_config: &ServiceConfig) {
    let _guards = init_logging(service_config);
    let (_, handle) = start_service(service_config).await;
    handle.wait().await;
}
