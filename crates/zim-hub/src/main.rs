use std::sync::Arc;

use tokio::sync::watch;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use zim_hub::config::Config;
use zim_hub::http::HttpServer;
use zim_hub::identity::IdentityStore;
use zim_hub::peer_client::PeerClient;
use zim_hub::state::AppState;
use zim_hub::{Service, ShutdownHandle};
use zim_peer::state::BlobStoreConfig;
use zim_peer::{ServiceConfig, ServiceState};

const DEFAULT_FILTER: &str = "info,\
    hyper=warn,\
    hyper_util=warn,\
    h2=warn,\
    tower=warn,\
    tokio_util=warn,\
    sqlx=warn";

fn init_logging(log_level: tracing::Level) -> Vec<tracing_appender::non_blocking::WorkerGuard> {
    let mut guards = Vec::new();

    let (stdout_writer, stdout_guard) = tracing_appender::non_blocking(std::io::stdout());
    guards.push(stdout_guard);

    let base = format!("{DEFAULT_FILTER},zim_hub={log_level},zim_peer=info,zim_protocol=info");
    let filter = EnvFilter::builder().parse_lossy(
        std::env::var("RUST_LOG")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or(base),
    );

    let layer = tracing_subscriber::fmt::layer()
        .with_writer(stdout_writer)
        .with_target(false)
        .with_thread_ids(false)
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .compact()
        .with_filter(filter);

    tracing_subscriber::registry().with(layer).init();

    std::panic::set_hook(Box::new(|panic| match panic.location() {
        Some(loc) => tracing::error!(
            message = %panic,
            panic.file = loc.file(),
            panic.line = loc.line(),
            panic.column = loc.column(),
        ),
        None => tracing::error!(message = %panic),
    }));

    guards
}

fn banner(config: &Config, node_id: &impl std::fmt::Display) {
    let version = env!("CARGO_PKG_VERSION");
    tracing::info!("─────────────────────────────────────────");
    tracing::info!("  zim-hub v{version}");
    tracing::info!("  listen   {}", config.listen_address);
    tracing::info!("  data     {}", config.data_dir.display());
    tracing::info!("  node     {node_id}");
    tracing::info!("  services http, peer (in-process / mirror)");
    tracing::info!("─────────────────────────────────────────");
    tracing::info!("");
    tracing::info!("To mirror a bucket on this hub, run on the owning peer:");
    tracing::info!("  zim bucket mirror add <BUCKET_ID> {node_id}");
    tracing::info!("");
}

#[tokio::main]
async fn main() {
    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("configuration error: {e}");
            std::process::exit(1);
        }
    };

    let _guards = init_logging(config.log_level);

    // Ensure data dir exists before constructing the embedded peer.
    if let Err(e) = std::fs::create_dir_all(&config.data_dir) {
        tracing::error!(
            "failed to create data dir {}: {e}",
            config.data_dir.display()
        );
        std::process::exit(2);
    }

    // Embedded peer config. We pin api_port/gateway_port to 0 because we do not
    // spawn the zim-peer HTTP servers — the hub serves HTTP itself, and the
    // peer is consumed in-process via ServiceState.
    //
    // ServiceState::from_config rejects a sqlite_path that doesn't exist on
    // disk, so we create an empty file here when this is the first launch;
    // sqlx then connects + runs migrations against the empty file.
    let sqlite_path = config.data_dir.join("zim-hub.db");
    if !sqlite_path.exists() {
        if let Err(e) = std::fs::File::create(&sqlite_path) {
            tracing::error!(
                "failed to create sqlite file {}: {e}",
                sqlite_path.display()
            );
            std::process::exit(2);
        }
    }
    let svc_cfg = ServiceConfig {
        node_listen_addr: None,
        node_secret: None,
        blob_store: BlobStoreConfig::default(),
        jax_dir: config.data_dir.clone(),
        max_import_size: 100 * 1024 * 1024,
        api_port: 0,
        gateway_port: 0,
        sqlite_path: Some(sqlite_path),
        log_level: config.log_level,
        log_dir: None,
        gateway_url: None,
    };

    let service = match ServiceState::from_config(&svc_cfg).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("failed to initialize embedded peer state: {e}");
            std::process::exit(3);
        }
    };

    // Banner deferred until after ServiceState init so we can include the
    // embedded peer's node id and a copy-pasteable `zim bucket mirror add`
    // command template (T-016d).
    banner(&config, &service.peer().id());

    // Hub-side identity store (T-001a M1). Separate SQLite DB from the
    // embedded peer; lives in the same data dir.
    let identity_path = config.data_dir.join("identity.db");
    let identity = match IdentityStore::open(&identity_path).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(
                "failed to open identity store at {}: {e}",
                identity_path.display()
            );
            std::process::exit(3);
        }
    };

    let state = AppState {
        listen_address: config.listen_address,
        peer: PeerClient::new(service.clone()),
        identity,
    };

    let (mut handle, shutdown_rx) = ShutdownHandle::new();

    // Spawn the in-process peer task (sync, blob serving, etc.).
    let peer_for_spawn = service.peer().clone();
    let peer_shutdown = shutdown_rx.clone();
    handle.push(
        "peer",
        tokio::spawn(async move {
            if let Err(e) = zim_protocol::spawn(peer_for_spawn, peer_shutdown).await {
                tracing::error!("embedded peer exited: {e}");
            }
        }),
    );

    // Spawn the HTTP server.
    handle.push(
        "http",
        HttpServer::spawn(state.clone(), shutdown_rx.clone()),
    );

    // Keep ServiceState alive for the lifetime of the process. AppState holds
    // a clone; this keeps the original around via an Arc-ish hold (Database
    // pool, peer, etc. are themselves cheaply cloneable, but holding the
    // original here is cheap insurance against premature drop).
    let _keep_service_alive = Arc::new(service);
    let _keep_shutdown_rx = watch::Receiver::clone(&shutdown_rx);

    handle.wait().await;
}
