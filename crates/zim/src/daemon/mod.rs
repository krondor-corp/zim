//! The daemon: runtime state + the axum HTTP surface.
//!
//! - [`state`] — [`ServiceState`]: boots the peer, blob store, mounts,
//!   resolver; everything long-lived the handlers share.
//! - [`accept`] — the contacts-backed [`zim_peer::AcceptPolicy`].
//! - [`config`] — listen addr + tracing level.
//! - [`api`] / [`health`] / [`handlers`] — the HTTP surface: `/api/v0`
//!   (typed daemon RPC — the CLI is a thin client over it), `/_status`,
//!   and a content-negotiated 404 fallback.

use axum::Router;
use tokio::sync::watch;
use tower_http::trace::TraceLayer;
use tower_http::trace::{DefaultOnFailure, DefaultOnResponse};
use tower_http::LatencyUnit;

pub mod accept;
pub mod api;
pub mod config;
pub mod handlers;
pub mod health;
pub mod state;

pub use config::Config;

pub use state::ServiceState;

/// 500 MB body limit — same as the reference; covers single-shot
/// file uploads through `/api/v0/vault/add`.
pub const MAX_UPLOAD_SIZE_BYTES: usize = 500 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum HttpServerError {
    #[error("listener bind: {0}")]
    Bind(#[from] std::io::Error),
}

/// Run the API HTTP server. Returns when `shutdown_rx` fires or the
/// listener errors.
pub async fn run_api(
    config: Config,
    state: ServiceState,
    mut shutdown_rx: watch::Receiver<()>,
) -> Result<(), HttpServerError> {
    let trace_layer = TraceLayer::new_for_http()
        .on_response(
            DefaultOnResponse::new()
                .include_headers(false)
                .level(config.log_level)
                .latency_unit(LatencyUnit::Micros),
        )
        .on_failure(DefaultOnFailure::new().latency_unit(LatencyUnit::Micros));

    let router: Router = Router::new()
        .nest("/_status", health::router(state.clone()))
        .nest("/api", api::router(state.clone()))
        .fallback(handlers::not_found::handler)
        .layer(tower_http::limit::RequestBodyLimitLayer::new(
            MAX_UPLOAD_SIZE_BYTES,
        ))
        .with_state(state)
        .layer(trace_layer);

    tracing::info!(addr = %config.listen_addr, "API server listening");
    let listener = tokio::net::TcpListener::bind(config.listen_addr).await?;

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.changed().await;
        })
        .await?;

    Ok(())
}
