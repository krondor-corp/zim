use askama::Template;
use axum::extract::{Path, State};
use axum::response::Html;
use uuid::Uuid;

use crate::errors::{Error, Result};
use crate::http::html::bucket::views::breadcrumb;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "pages/bucket/blob.html")]
struct BlobTemplate {
    bucket_id: Uuid,
    path: String,
    mime_type: String,
    size: usize,
    breadcrumb: Vec<(String, Option<String>)>,
    /// UTF-8 text rendering when mime_type begins with `text/` or is a known
    /// text-y format; `None` otherwise (binary blob — show a download prompt).
    text: Option<String>,
}

pub async fn handler(
    State(state): State<AppState>,
    Path((bucket_id, path)): Path<(Uuid, String)>,
) -> Result<Html<String>> {
    let api_path = format!("/{path}");
    let result = state.peer.cat(bucket_id, &api_path).await?;
    let text = render_text(&result.mime_type, &result.bytes);
    let tmpl = BlobTemplate {
        bucket_id,
        path,
        mime_type: result.mime_type,
        size: result.size,
        breadcrumb: breadcrumb(bucket_id, &result.path),
        text,
    };
    Ok(Html(tmpl.render().map_err(Error::Template)?))
}

fn render_text(mime: &str, bytes: &[u8]) -> Option<String> {
    let texty = mime.starts_with("text/")
        || matches!(
            mime,
            "application/json"
                | "application/xml"
                | "application/yaml"
                | "application/x-yaml"
                | "application/javascript"
                | "application/toml"
        );
    if !texty {
        return None;
    }
    std::str::from_utf8(bytes).ok().map(str::to_string)
}
