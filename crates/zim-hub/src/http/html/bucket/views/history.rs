use askama::Template;
use axum::extract::{Path, State};
use axum::response::Html;
use uuid::Uuid;

use crate::errors::{Error, Result};
use crate::http::html::bucket::views::breadcrumb;
use crate::peer_client::HistoryEntry;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "pages/bucket/history.html")]
struct HistoryTemplate {
    bucket_id: Uuid,
    breadcrumb: Vec<(String, Option<String>)>,
    entries: Vec<HistoryEntry>,
}

pub async fn handler(
    State(state): State<AppState>,
    Path(bucket_id): Path<Uuid>,
) -> Result<Html<String>> {
    let entries = state.peer.history(bucket_id, 0, 50).await?;
    let mut crumbs = breadcrumb(bucket_id, "");
    crumbs.push(("history".to_string(), None));
    let tmpl = HistoryTemplate {
        bucket_id,
        breadcrumb: crumbs,
        entries,
    };
    Ok(Html(tmpl.render().map_err(Error::Template)?))
}
