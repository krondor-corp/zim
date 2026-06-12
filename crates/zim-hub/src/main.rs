use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use zim_hub::config::Config;
use zim_hub::http::HttpServer;
use zim_hub::state::AppState;
use zim_hub::{Service, ShutdownHandle};

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

    let base = format!("{DEFAULT_FILTER},zim_hub={log_level},zim=info");
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

fn banner(config: &Config) {
    let version = env!("CARGO_PKG_VERSION");
    let did = config.did();
    tracing::info!("─────────────────────────────────────────");
    tracing::info!("  zim-hub v{version}");
    tracing::info!("  listen   {}", config.listen_address);
    tracing::info!("  home     {}", config.data_dir.display());
    tracing::info!("  did      {did}");
    tracing::info!(
        "  doc      http://{}/.well-known/did.json",
        config.listen_address
    );
    tracing::info!("  services http, peer (in-process / relay)");
    tracing::info!("─────────────────────────────────────────");
    tracing::info!("");
    tracing::info!("To mirror a vault on this hub, run on the owning peer:");
    tracing::info!("  zim peers add hub {did}");
    tracing::info!("  zim vault <vault-id> relays add hub");
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

    if let Err(e) = std::fs::create_dir_all(&config.data_dir) {
        tracing::error!(
            "failed to create data dir {}: {e}",
            config.data_dir.display()
        );
        std::process::exit(2);
    }

    // Blob store: S3-compatible object store (minio in dev) when
    // `ZIM_HUB_S3_*` is configured, local filesystem store otherwise.
    // The SQLite index that maps blake3 hashes onto object keys
    // always lives in the data dir.
    let service = match &config.s3 {
        Some(s3) => {
            tracing::info!("  blobs    s3 {} bucket={}", s3.endpoint, s3.bucket);
            let index_path = config.data_dir.join("blob-index.sqlite");
            let blobs = match zim_peer::object_store::s3_provider(
                &index_path,
                &s3.endpoint,
                &s3.access_key,
                &s3.secret_key,
                &s3.bucket,
                s3.region.as_deref(),
            )
            .await
            {
                Ok(b) => b,
                Err(e) => {
                    tracing::error!("failed to open s3 blob store: {e}");
                    std::process::exit(3);
                }
            };
            zim::ServiceState::boot_with_blobs(&config.data_dir, blobs).await
        }
        None => {
            tracing::info!("  blobs    local fs (set ZIM_HUB_S3_* for object storage)");
            zim::ServiceState::boot(&config.data_dir).await
        }
    };
    let service = match service {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("failed to boot embedded peer: {e}");
            std::process::exit(3);
        }
    };

    // Hub-local state DB. Co-located with the peer's data dir but in
    // its own file so a `rm state/hub.db` only wipes hub-app state
    // (users, peers, escrow) and leaves the vault mirror intact.
    // URL form so the same code accepts `postgres://…` when we go
    // multi-node.
    let state_dir = config.data_dir.join("state");
    if let Err(e) = std::fs::create_dir_all(&state_dir) {
        tracing::error!("failed to create state dir {}: {e}", state_dir.display());
        std::process::exit(4);
    }
    let hub_db_path = state_dir.join("hub.db");
    // sqlx's SqliteConnectOptions::from_url wants `sqlite:<path>`.
    // We canonicalize via `std::fs::canonicalize` so the URL has an
    // absolute path; if the file doesn't exist yet, canonicalize the
    // parent and re-append the filename.
    let abs_path = match std::fs::canonicalize(&state_dir) {
        Ok(parent) => parent.join("hub.db"),
        Err(e) => {
            tracing::error!("failed to canonicalize {}: {e}", state_dir.display());
            std::process::exit(4);
        }
    };
    let db_url = match url::Url::parse(&format!("sqlite://{}", abs_path.display())) {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("failed to build sqlite URL for {}: {e}", abs_path.display());
            std::process::exit(4);
        }
    };
    tracing::info!("hub db at {}", abs_path.display());
    let _ = hub_db_path; // silence unused warning
    let db = match zim_hub::Database::connect(&db_url).await {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("hub db setup failed: {e}");
            std::process::exit(4);
        }
    };

    banner(&config);

    let app_state = AppState::new(&config, service.clone(), db);

    let (mut handle, shutdown_rx) = ShutdownHandle::new();

    // The embedded peer's sync loop runs as its own task. ServiceState
    // is clone-cheap (Peer is Arc-wrapped internally).
    let peer = service.peer().clone();
    let peer_task = peer.spawn(shutdown_rx.clone());
    handle.push("peer", peer_task);

    handle.push("http", HttpServer::spawn(app_state, shutdown_rx));

    handle.wait().await;
}
