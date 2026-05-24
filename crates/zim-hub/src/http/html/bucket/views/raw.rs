use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use base64::Engine;
use uuid::Uuid;

use crate::errors::{Error, Result};
use crate::state::AppState;

pub async fn handler(
    State(state): State<AppState>,
    Path((bucket_id, path)): Path<(Uuid, String)>,
) -> Result<Response> {
    let api_path = format!("/{path}");
    let response = state.peer.cat(bucket_id, &api_path, None).await?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&response.content)
        .map_err(|e| Error::Decode(e.to_string()))?;

    let filename = std::path::Path::new(&response.path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download");

    Ok((
        [
            (header::CONTENT_TYPE, response.mime_type),
            (header::CONTENT_LENGTH, bytes.len().to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"{filename}\""),
            ),
        ],
        bytes,
    )
        .into_response())
}
