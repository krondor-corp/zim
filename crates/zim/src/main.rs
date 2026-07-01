//! `zim` CLI entry point — parse args, run the top-level Op,
//! render output, exit with the right code.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use tracing_appender::non_blocking::WorkerGuard;

use zim::cli::args::Args;
use zim::cli::op::Op;
use zim::cli::ops::daemon::Daemon;
use zim::cli::ui;
use zim::context::paths;
use zim::Command;

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();

    if !std::io::stdout().is_terminal() {
        owo_colors::set_override(false);
    }

    // Pick the log file path *before* installing the subscriber.
    // Only `zim daemon run` writes to disk; CLI ops are too
    // short-lived to be worth logging.
    let log_path = daemon_log_path(&args);
    let _log_guard = init_tracing(log_path);

    match args.command.run(()).await {
        Ok(output) => {
            if args.plain {
                // `--plain` → machine-readable JSON. Skip unit-shaped
                // outputs (action ops with nothing meaningful to
                // emit) so consumers don't have to special-case
                // empty bodies.
                let json =
                    serde_json::to_string_pretty(&output).expect("OpOutput serialize never fails");
                if json != "null" && json != "{}" {
                    println!("{json}");
                }
            } else {
                print!("{output}");
                if !format!("{output}").ends_with('\n') {
                    println!();
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{}", ui::format_error(&e));
            ExitCode::from(1)
        }
    }
}

/// `Some(path)` when this invocation is `zim daemon run`, meaning we
/// should tee tracing output to `$ZIM_HOME/state/daemon.log` so the
/// service-managed daemon (whose stderr launchd discards) doesn't
/// lose anything. Returns `None` for every other command.
fn daemon_log_path(args: &Args) -> Option<PathBuf> {
    if !matches!(args.command, Command::Daemon(Daemon::Run(_))) {
        return None;
    }
    let home = paths::home_dir(args.config_path.as_deref()).ok()?;
    paths::ensure_dirs(&home).ok()?;
    Some(paths::daemon_log_path(&home))
}

/// Install the tracing subscriber. Stderr always, and `daemon.log`
/// too when `log_path` is set. Returns the appender's worker guard,
/// which must be held for the process lifetime so buffered log
/// records flush on shutdown.
fn init_tracing(log_path: Option<PathBuf>) -> Option<WorkerGuard> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{fmt, EnvFilter, Layer};

    // Default: all our crates at INFO (DEBUG in debug builds),
    // everything else at WARN. `zim=info` alone would silence
    // `zim_peer` / `zim_core` — exactly the modules doing the
    // interesting background work (pulls, share-offers, announces).
    // Override via `ZIM_LOG`, e.g. `ZIM_LOG=zim=trace,zim_peer=trace`.
    const DEFAULT_FILTER: &str = if cfg!(debug_assertions) {
        "zim=debug,zim_peer=debug,zim_core=debug,zim_crypto=debug,warn"
    } else {
        "zim=info,zim_peer=info,zim_core=info,zim_crypto=info,warn"
    };
    let filter =
        || EnvFilter::try_from_env("ZIM_LOG").unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(filter());

    let (file_layer, guard) = match log_path.as_ref() {
        Some(path) => {
            let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
            let file_name = path.file_name().expect("log path has file name");
            let appender = tracing_appender::rolling::never(parent, file_name);
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let layer = fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .with_filter(filter());
            (Some(layer), Some(guard))
        }
        None => (None, None),
    };

    let _ = tracing_subscriber::registry()
        .with(stderr_layer)
        .with(file_layer)
        .try_init();

    guard
}
