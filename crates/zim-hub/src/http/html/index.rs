use askama::Template;
use axum::extract::State;
use axum::response::Html;

use crate::errors::Result;
use crate::peer_client::BucketInfo;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "pages/index.html")]
struct IndexTemplate {
    buckets: Vec<BucketInfo>,
}

pub async fn handler(State(state): State<AppState>) -> Result<Html<String>> {
    let mut buckets = state.peer.list_buckets().await?;
    buckets.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    let tmpl = IndexTemplate { buckets };
    Ok(Html(tmpl.render().unwrap_or_default()))
}
