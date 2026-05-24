use askama::Template;
use axum::extract::State;
use axum::response::Html;

use crate::errors::Result;
use crate::peer_client::BucketInfo;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "pages/index.html")]
struct IndexTemplate {
    /// `Some(buckets)` if the peer was reachable; `None` if it was not (we
    /// still render the page with a friendly status note).
    buckets: Option<Vec<BucketInfo>>,
    /// Human-readable peer endpoint, shown in the offline note.
    peer_endpoint: String,
    /// Error string when buckets is `None`.
    peer_error: Option<String>,
}

pub async fn handler(State(state): State<AppState>) -> Result<Html<String>> {
    let peer_endpoint = state.peer.base().to_string();
    let (buckets, peer_error) = match state.peer.list_buckets().await {
        Ok(resp) => {
            let mut bs = resp.buckets;
            bs.sort_by_key(|b| std::cmp::Reverse(b.created_at));
            (Some(bs), None)
        }
        Err(e) => {
            tracing::warn!("peer list_buckets failed: {e}");
            (None, Some(e.to_string()))
        }
    };
    let tmpl = IndexTemplate {
        buckets,
        peer_endpoint,
        peer_error,
    };
    Ok(Html(tmpl.render().unwrap_or_default()))
}
