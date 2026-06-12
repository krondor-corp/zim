//! Process utilities used by the daemon entrypoint.
//!
//! `graceful_shutdown_blocker` used to live here; the SIGINT/SIGTERM listener
//! is now part of [`zim_runtime::ShutdownHandle`] and shared with `zim-hub`.

/// Registers a panic hook that logs panics using the `tracing` crate
pub fn register_panic_logger() {
    std::panic::set_hook(Box::new(|panic| match panic.location() {
        Some(loc) => {
            tracing::error!(
                message = %panic,
                panic.file = loc.file(),
                panic.line = loc.line(),
                panic.column = loc.column(),
            );
        }
        None => tracing::error!(message = %panic),
    }));
}

pub fn report_build_info() {
    let build = crate::build_info();

    tracing::info!(
        build_profile = ?build.build_profile,
        features = ?build.build_features,
        version = ?build.version,
        "service starting up"
    );
}
