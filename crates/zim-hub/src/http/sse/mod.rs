use axum::Router;

use crate::state::AppState;

/// Reserved namespace for Datastar SSE merge-fragment streams.
/// First consumer: bucket change notifications under `/_events/b/{id}/changes`.
pub fn router(state: AppState) -> Router<AppState> {
    Router::new().with_state(state)
}
