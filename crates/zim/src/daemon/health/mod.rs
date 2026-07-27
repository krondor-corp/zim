//! `/_status` — health + identity endpoints.
//!
//! GET-only, JSON. CORS allows any origin (probes are by design
//! reachable from monitoring tools and other peers). Body limit
//! `HEALTHCHECK_REQUEST_SIZE_LIMIT` rejects oversized requests.

use axum::routing::get;
use axum::Router;
use http::header::{ACCEPT, ORIGIN};
use http::Method;
use tower_http::cors::{Any, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;

pub mod data_source;
pub mod identity;
pub mod liveness;
pub mod readiness;
pub mod version;

use crate::daemon::state::ServiceState;

/// Probes shouldn't carry real bodies — guard against accidental
/// large payloads to these endpoints.
const HEALTHCHECK_REQUEST_SIZE_LIMIT: usize = 1_024;

pub fn router(state: ServiceState) -> Router<ServiceState> {
    let cors = CorsLayer::new()
        .allow_methods(vec![Method::GET])
        .allow_headers(vec![ACCEPT, ORIGIN])
        .allow_origin(Any)
        .allow_credentials(false);

    Router::new()
        .route("/livez", get(liveness::handler))
        .route("/readyz", get(readiness::handler))
        .route("/version", get(version::handler))
        .route("/identity", get(identity::handler))
        .with_state(state)
        .layer(cors)
        .layer(RequestBodyLimitLayer::new(HEALTHCHECK_REQUEST_SIZE_LIMIT))
}
