pub mod api;
pub mod auth;
pub mod health;
pub mod html;
pub mod spa;
pub mod user_did;
pub mod well_known;

use axum::http::HeaderValue;
use axum::routing::get;
use axum::Router;
use tokio::sync::watch;
use tower_http::cors::{Any, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

/// Content Security Policy applied to every response.
///
/// `script-src` is `'self'` plus `'wasm-unsafe-eval'` (the Yew/WASM bundle
/// instantiates its module) and `'unsafe-inline'` for the small inline
/// theme-init script. No `'unsafe-eval'` — Datastar (which needed it) is gone.
const CSP: &str = "default-src 'self'; \
                   script-src 'self' 'wasm-unsafe-eval' 'unsafe-inline'; \
                   style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; \
                   font-src 'self' https://fonts.gstatic.com; \
                   img-src 'self' data: blob:; \
                   object-src 'none'; \
                   base-uri 'self'; \
                   frame-ancestors 'none'";

pub struct HttpServer;

pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .allow_origin(Any);

    Router::new()
        // Operator / infra surface (unauthenticated).
        .nest("/_status", health::router(state.clone()))
        .nest("/.well-known", well_known::router(state.clone()))
        // Per-user did:web documents — `did:web:<host>:u:<id>` resolves here.
        // Public (pubkeys only), like the hub's own did.json.
        .nest("/u", user_did::router(state.clone()))
        .route("/static/*path", get(html::static_files::handler))
        // OAuth flow is public so anonymous users can sign in.
        .nest("/auth", auth::router(state.clone()))
        // JSON API — the SPA's whole backend (incl. /api/v0/admin).
        .nest("/api", api::router(state.clone()))
        // The Yew SPA owns everything else: `/`, `/v/:id`, `/settings`,
        // `/admin`, `/device`, deep links, and its hashed assets. Mounted last
        // as the fallback so the routes above win.
        .route("/", get(spa::root))
        .fallback(spa::fallback)
        .with_state(state)
        .layer(SetResponseHeaderLayer::if_not_present(
            axum::http::header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CSP),
        ))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}

#[async_trait::async_trait]
impl zim_runtime::Service for HttpServer {
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
