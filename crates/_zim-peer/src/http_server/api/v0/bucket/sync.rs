//! Filesystem backup sync target management endpoints (T-018).
//!
//! CRUD for `sync_targets` table — register/remove/list/pause/resume backup
//! targets. The actual sync logic (tree-diff + materialize) runs in a
//! `SyncService` background worker (separate from this HTTP surface).

use axum::extract::{Json, State};
use axum::response::{IntoResponse, Response};
use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::database::models::sync_target::{SyncStatus, SyncTarget};
use crate::http_server::api::client::ApiRequest;
use crate::ServiceState;

// =============================================================
// add
// =============================================================

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct AddSyncRequest {
    #[arg(long)]
    pub bucket_id: Uuid,
    #[arg(long)]
    pub target_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddSyncResponse {
    pub id: Uuid,
    pub bucket_id: Uuid,
    pub target_path: String,
}

pub async fn add_handler(
    State(state): State<ServiceState>,
    Json(req): Json<AddSyncRequest>,
) -> Result<impl IntoResponse, SyncError> {
    let target = SyncTarget::create(req.bucket_id, &req.target_path, state.database())?;
    Ok((
        http::StatusCode::OK,
        Json(AddSyncResponse {
            id: target.id,
            bucket_id: target.bucket_id,
            target_path: target.target_path,
        }),
    )
        .into_response())
}

// =============================================================
// remove
// =============================================================

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct RemoveSyncRequest {
    #[arg(long)]
    pub bucket_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveSyncResponse {
    pub removed: bool,
}

pub async fn remove_handler(
    State(state): State<ServiceState>,
    Json(req): Json<RemoveSyncRequest>,
) -> Result<impl IntoResponse, SyncError> {
    let removed = SyncTarget::remove_by_bucket(req.bucket_id, state.database())?;
    Ok((http::StatusCode::OK, Json(RemoveSyncResponse { removed })).into_response())
}

// =============================================================
// list
// =============================================================

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct ListSyncRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSyncResponse {
    pub targets: Vec<SyncTargetInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncTargetInfo {
    pub id: Uuid,
    pub bucket_id: Uuid,
    pub target_path: String,
    pub last_head: Option<String>,
    pub last_sync: Option<i64>,
    pub status: SyncStatus,
    pub error_message: Option<String>,
}

pub async fn list_handler(
    State(state): State<ServiceState>,
    Json(_req): Json<ListSyncRequest>,
) -> Result<impl IntoResponse, SyncError> {
    let targets = SyncTarget::list(state.database())?;
    let infos = targets
        .into_iter()
        .map(|t| SyncTargetInfo {
            id: t.id,
            bucket_id: t.bucket_id,
            target_path: t.target_path,
            last_head: t.last_head,
            last_sync: t.last_sync,
            status: t.status,
            error_message: t.error_message,
        })
        .collect();
    Ok((
        http::StatusCode::OK,
        Json(ListSyncResponse { targets: infos }),
    )
        .into_response())
}

// =============================================================
// pause / resume
// =============================================================

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
pub struct PauseSyncRequest {
    #[arg(long)]
    pub bucket_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusChangeResponse {
    pub bucket_id: Uuid,
    pub new_status: SyncStatus,
}

pub async fn pause_handler(
    State(state): State<ServiceState>,
    Json(req): Json<PauseSyncRequest>,
) -> Result<impl IntoResponse, SyncError> {
    let targets = SyncTarget::list_by_bucket(req.bucket_id, state.database())?;
    for t in &targets {
        SyncTarget::set_status(t.id, SyncStatus::Paused, None, state.database())?;
    }
    Ok((
        http::StatusCode::OK,
        Json(StatusChangeResponse {
            bucket_id: req.bucket_id,
            new_status: SyncStatus::Paused,
        }),
    )
        .into_response())
}

pub async fn resume_handler(
    State(state): State<ServiceState>,
    Json(req): Json<PauseSyncRequest>,
) -> Result<impl IntoResponse, SyncError> {
    let targets = SyncTarget::list_by_bucket(req.bucket_id, state.database())?;
    for t in &targets {
        SyncTarget::set_status(t.id, SyncStatus::Active, None, state.database())?;
    }
    Ok((
        http::StatusCode::OK,
        Json(StatusChangeResponse {
            bucket_id: req.bucket_id,
            new_status: SyncStatus::Active,
        }),
    )
        .into_response())
}

// =============================================================
// error
// =============================================================

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("Database error: {0}")]
    Db(#[from] crate::database::DatabaseError),
}

impl IntoResponse for SyncError {
    fn into_response(self) -> Response {
        (
            http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Sync target error: {}", self),
        )
            .into_response()
    }
}

// =============================================================
// client builders
// =============================================================

impl ApiRequest for AddSyncRequest {
    type Response = AddSyncResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        client
            .post(base_url.join("/api/v0/bucket/sync/add").unwrap())
            .json(&self)
    }
}

impl ApiRequest for RemoveSyncRequest {
    type Response = RemoveSyncResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        client
            .post(base_url.join("/api/v0/bucket/sync/remove").unwrap())
            .json(&self)
    }
}

impl ApiRequest for ListSyncRequest {
    type Response = ListSyncResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        client
            .post(base_url.join("/api/v0/bucket/sync/list").unwrap())
            .json(&self)
    }
}

impl ApiRequest for PauseSyncRequest {
    type Response = StatusChangeResponse;
    fn build_request(self, base_url: &Url, client: &Client) -> RequestBuilder {
        client
            .post(base_url.join("/api/v0/bucket/sync/pause").unwrap())
            .json(&self)
    }
}
