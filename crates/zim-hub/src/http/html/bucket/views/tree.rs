use askama::Template;
use axum::extract::{Path, State};
use axum::response::Html;
use uuid::Uuid;

use crate::errors::{Error, Result};
use crate::http::html::bucket::views::breadcrumb;
use crate::peer_client::PathInfo;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "pages/bucket/tree.html")]
struct TreeTemplate {
    bucket_id: Uuid,
    path: String,
    breadcrumb: Vec<(String, Option<String>)>,
    items: Vec<PathInfo>,
}

pub async fn handler_root(
    State(state): State<AppState>,
    Path(bucket_id): Path<Uuid>,
) -> Result<Html<String>> {
    render(state, bucket_id, String::new()).await
}

pub async fn handler(
    State(state): State<AppState>,
    Path((bucket_id, path)): Path<(Uuid, String)>,
) -> Result<Html<String>> {
    render(state, bucket_id, path).await
}

async fn render(state: AppState, bucket_id: Uuid, path: String) -> Result<Html<String>> {
    let api_path = if path.is_empty() {
        None
    } else {
        Some(format!("/{path}"))
    };
    let response = state.peer.ls(bucket_id, api_path.as_deref(), None).await?;
    let mut items = response.items;
    items.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });
    let crumbs = breadcrumb(bucket_id, &path);
    let tmpl = TreeTemplate {
        bucket_id,
        path,
        breadcrumb: crumbs,
        items,
    };
    Ok(Html(tmpl.render().map_err(Error::Template)?))
}
