use askama::Template;
use axum::extract::{Path, State};
use axum::response::Html;
use base64::Engine;
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
    let response = state.peer.cat(bucket_id, &api_path, None).await?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&response.content)
        .map_err(|e| Error::Decode(e.to_string()))?;

    let text = render_text(&response.mime_type, &bytes);

    let tmpl = BlobTemplate {
        bucket_id,
        path,
        mime_type: response.mime_type,
        size: response.size,
        breadcrumb: breadcrumb(bucket_id, &response.path),
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
