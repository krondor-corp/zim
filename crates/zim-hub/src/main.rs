use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use zim_hub::config::Config;
use zim_hub::http::HttpServer;
use zim_hub::runtime::{Service, ShutdownHandle};
use zim_hub::state::AppState;

const DEFAULT_FILTER: &str = "info,\
    hyper=warn,\
    hyper_util=warn,\
    h2=warn,\
    tower=warn,\
    tokio_util=warn";

fn init_logging(log_level: tracing::Level) -> Vec<tracing_appender::non_blocking::WorkerGuard> {
    let mut guards = Vec::new();

    let (stdout_writer, stdout_guard) = tracing_appender::non_blocking(std::io::stdout());
    guards.push(stdout_guard);

    let base = format!("{DEFAULT_FILTER},zim_hub={log_level}");
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
    tracing::info!("─────────────────────────────────────────");
    tracing::info!("  zim-hub v{version}");
    tracing::info!("  listen   {}", config.listen_address);
    tracing::info!("  services http");
    tracing::info!("─────────────────────────────────────────");
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
    banner(&config);

    let state = AppState::from_config(&config);
    let (mut handle, shutdown_rx) = ShutdownHandle::new();

    handle.push(
        "http",
        HttpServer::spawn(state.clone(), shutdown_rx.clone()),
    );

    handle.wait().await;
}
