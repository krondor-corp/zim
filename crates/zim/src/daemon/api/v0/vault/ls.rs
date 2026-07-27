use std::path::Path;

use axum::response::IntoResponse;
use axum::Json;
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_core::fs::AbsPath;

use crate::daemon::api::client::ApiRequest;
use crate::daemon::api::v0::vault::extractor::VaultHandle;
use crate::daemon::state::vault_lookup_response;
use zim_peer::VaultLookupError;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LsRequest {
    #[serde(skip)]
    pub vault_id: zim_core::vault::VaultId,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LsResponse {
    pub path: String,
    pub items: Vec<PathInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathInfo {
    pub name: String,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Dir,
}

pub async fn handler(
    VaultHandle { vault, .. }: VaultHandle,
    Json(req): Json<LsRequest>,
) -> Result<impl IntoResponse, LsError> {
    let abs = AbsPath::new(&req.path).ok_or_else(|| LsError::BadPath(req.path.clone()))?;
    let entries = vault
        .fs()
        .ls(&abs)
        .await
        .map_err(|e| LsError::Load(e.to_string()))?;

    let items = entries
        .into_iter()
        .map(|(p, entry)| PathInfo {
            name: path_name(&p),
            kind: if entry.is_dir() {
                EntryKind::Dir
            } else {
                EntryKind::File
            },
        })
        .collect();

    Ok((
        http::StatusCode::OK,
        Json(LsResponse {
            path: req.path,
            items,
        }),
    )
        .into_response())
}

fn path_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| p.display().to_string())
}

#[derive(Debug, thiserror::Error)]
pub enum LsError {
    #[error(transparent)]
    Vault(#[from] VaultLookupError),
    #[error("invalid abs path: {0}")]
    BadPath(String),
    #[error("load: {0}")]
    Load(String),
}

impl IntoResponse for LsError {
    fn into_response(self) -> axum::response::Response {
        match self {
            LsError::Vault(e) => vault_lookup_response(e),
            LsError::BadPath(_) => {
                (http::StatusCode::BAD_REQUEST, self.to_string()).into_response()
            }
            LsError::Load(_) => {
                (http::StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
            }
        }
    }
}

impl ApiRequest for LsRequest {
    type Response = LsResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        let url = base_url
            .join(&format!("/api/v0/vaults/{}/ls", self.vault_id))
            .unwrap();
        client.post(url).json(&self)
    }
}
