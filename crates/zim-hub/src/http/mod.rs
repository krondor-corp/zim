pub mod health;
pub mod html;
pub mod sse;

use axum::Router;
use tokio::sync::watch;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::runtime;
use crate::state::AppState;

pub struct HttpServer;

pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .allow_origin(Any);

    Router::new()
        .nest("/_status", health::router(state.clone()))
        .nest("/_events", sse::router(state.clone()))
        .merge(html::router(state.clone()))
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}

#[async_trait::async_trait]
impl runtime::Service for HttpServer {
    type State = AppState;

    async fn run(state: Self::State, mut shutdown_rx: watch::Receiver<()>) {
        let addr = state.listen_address;
        let app = build_router(state);

        tracing::info!("listening on {addr}");

        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                tracing::error!(
                    "failed to bind {addr}: {e} — another process is holding the port. \
                     free it with: lsof -ti :{} | xargs kill",
                    addr.port()
                );
                return;
            }
            Err(e) => {
                tracing::error!("failed to bind {addr}: {e}");
                return;
            }
        };

        if let Err(e) = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.changed().await;
            })
            .await
        {
            tracing::error!("server error: {e}");
        }
    }
}
