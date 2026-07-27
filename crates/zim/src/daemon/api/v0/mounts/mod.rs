//! `/api/v0/mounts` — FUSE mount management.
//!
//! The request/response types + their [`ApiRequest`] impls are always
//! compiled so the CLI (a thin HTTP client) builds regardless of features.
//! The axum handlers + router are behind `feature = "fuse"` — without it the
//! routes simply aren't registered and the CLI gets a 404.

use reqwest::{Client, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use zim_core::vault::VaultId;

use crate::daemon::api::client::ApiRequest;

/// One mount registration + whether it's currently live.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountInfo {
    pub vault_id: VaultId,
    /// The vault's display name, if the daemon could resolve it. Shown in
    /// `mount list`; addressing a mount by name resolves through the vault
    /// name → id path, not this field.
    #[serde(default)]
    pub name: Option<String>,
    pub mountpoint: String,
    pub read_only: bool,
    pub auto_mount: bool,
    pub mounted: bool,
}

// -- list --------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse {
    pub mounts: Vec<MountInfo>,
}

impl ApiRequest for ListRequest {
    type Response = ListResponse;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client
            .post(base.join("/api/v0/mounts/list").unwrap())
            .json(&self)
    }
}

// -- add ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddRequest {
    pub vault_id: VaultId,
    pub mountpoint: String,
    #[serde(default)]
    pub auto_mount: bool,
    #[serde(default)]
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddResponse {
    pub mount: MountInfo,
}

impl ApiRequest for AddRequest {
    type Response = AddResponse;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client
            .post(base.join("/api/v0/mounts/add").unwrap())
            .json(&self)
    }
}

// -- set ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetRequest {
    pub mountpoint: String,
    /// `None` = leave unchanged.
    #[serde(default)]
    pub auto_mount: Option<bool>,
    #[serde(default)]
    pub read_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetResponse {
    pub mount: MountInfo,
}

impl ApiRequest for SetRequest {
    type Response = SetResponse;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client
            .post(base.join("/api/v0/mounts/set").unwrap())
            .json(&self)
    }
}

// -- stop --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopRequest {
    pub mountpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopResponse {
    pub stopped: bool,
}

impl ApiRequest for StopRequest {
    type Response = StopResponse;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client
            .post(base.join("/api/v0/mounts/stop").unwrap())
            .json(&self)
    }
}

// -- remove ------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveRequest {
    pub mountpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveResponse {
    pub removed: bool,
}

impl ApiRequest for RemoveRequest {
    type Response = RemoveResponse;
    fn build_request(self, base: &Url, client: &Client) -> RequestBuilder {
        client
            .post(base.join("/api/v0/mounts/remove").unwrap())
            .json(&self)
    }
}

// -- handlers (daemon-side, fuse only) ---------------------------------------

#[cfg(feature = "fuse")]
mod handlers {
    use std::path::PathBuf;

    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use axum::routing::post;
    use axum::{Json, Router};

    use super::*;
    use crate::ServiceState;

    pub fn router(state: ServiceState) -> Router<ServiceState> {
        Router::new()
            .route("/list", post(list))
            .route("/add", post(add))
            .route("/set", post(set))
            .route("/stop", post(stop))
            .route("/remove", post(remove))
            .with_state(state)
    }

    fn info(s: crate::mount::MountStatus, name: Option<String>) -> MountInfo {
        MountInfo {
            vault_id: s.vault_id,
            name,
            mountpoint: s.mountpoint.to_string_lossy().into_owned(),
            read_only: s.read_only,
            auto_mount: s.auto_mount,
            mounted: s.mounted,
        }
    }

    /// `vault_id → display name` for every vault the daemon can open. Used to
    /// label mounts; a vault that can't be opened just has no name.
    async fn vault_names(state: &ServiceState) -> std::collections::HashMap<VaultId, String> {
        state
            .peer()
            .list_vaults()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| v.name.map(|n| (v.id, n)))
            .collect()
    }

    async fn list(State(state): State<ServiceState>, Json(_req): Json<ListRequest>) -> Response {
        let names = vault_names(&state).await;
        let mounts = state
            .mounts()
            .list()
            .into_iter()
            .map(|s| {
                let name = names.get(&s.vault_id).cloned();
                info(s, name)
            })
            .collect();
        Json(ListResponse { mounts }).into_response()
    }

    async fn add(State(state): State<ServiceState>, Json(req): Json<AddRequest>) -> Response {
        let mountpoint = PathBuf::from(&req.mountpoint);
        match state
            .mounts()
            .add(req.vault_id, mountpoint, req.auto_mount, req.read_only)
            .await
        {
            Ok(()) => {
                let name = vault_names(&state).await.get(&req.vault_id).cloned();
                Json(AddResponse {
                    mount: MountInfo {
                        vault_id: req.vault_id,
                        name,
                        mountpoint: req.mountpoint,
                        read_only: req.read_only,
                        auto_mount: req.auto_mount,
                        mounted: true,
                    },
                })
                .into_response()
            }
            Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        }
    }

    async fn set(State(state): State<ServiceState>, Json(req): Json<SetRequest>) -> Response {
        match state
            .mounts()
            .set(
                &PathBuf::from(&req.mountpoint),
                req.auto_mount,
                req.read_only,
            )
            .await
        {
            Ok(status) => {
                let name = vault_names(&state).await.get(&status.vault_id).cloned();
                Json(SetResponse {
                    mount: info(status, name),
                })
                .into_response()
            }
            Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        }
    }

    async fn stop(State(state): State<ServiceState>, Json(req): Json<StopRequest>) -> Response {
        match state.mounts().stop(&PathBuf::from(&req.mountpoint)) {
            Ok(()) => Json(StopResponse { stopped: true }).into_response(),
            Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
        }
    }

    async fn remove(State(state): State<ServiceState>, Json(req): Json<RemoveRequest>) -> Response {
        match state.mounts().remove(&PathBuf::from(&req.mountpoint)) {
            Ok(removed) => Json(RemoveResponse { removed }).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    }
}

#[cfg(feature = "fuse")]
pub use handlers::router;
