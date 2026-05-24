use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::errors::Result;
use crate::state::AppState;

pub async fn handler(
    State(state): State<AppState>,
    Path((bucket_id, path)): Path<(Uuid, String)>,
) -> Result<Response> {
    let api_path = format!("/{path}");
    let result = state.peer.cat(bucket_id, &api_path).await?;

    let filename = std::path::Path::new(&result.path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download");

    Ok((
        [
            (header::CONTENT_TYPE, result.mime_type),
            (header::CONTENT_LENGTH, result.bytes.len().to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"{filename}\""),
            ),
        ],
        result.bytes,
    )
        .into_response())
}
