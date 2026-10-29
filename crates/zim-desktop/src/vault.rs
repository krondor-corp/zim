//! Vault commands: each proxies one daemon endpoint through the typed
//! client — the same requests the CLI sends. Arguments and results are
//! the shared [`zim_api::desktop`] types. File bytes cross the IPC
//! boundary raw (`tauri::ipc::Response` out, request body in), never as
//! JSON number arrays.

use tauri::ipc::{InvokeBody, Request, Response};
use tauri::State;
use zim::daemon::api::v0::vault::add::AddRequest;
use zim::daemon::api::v0::vault::cat::CatRequest;
use zim::daemon::api::v0::vault::create::CreateRequest;
use zim::daemon::api::v0::vault::head::HeadRequest;
use zim::daemon::api::v0::vault::list::ListRequest;
use zim::daemon::api::v0::vault::ls::LsRequest;
use zim::daemon::api::v0::vault::mkdir::MkdirRequest;
use zim::daemon::api::v0::vault::mv::MvRequest;
use zim::daemon::api::v0::vault::rm::RmRequest;
use zim_api::desktop::{
    AddHeaders, CommandError, CreateVault, Head, VaultMove, VaultPath, VaultRef, VaultSummary,
};
use zim_core::fs::{AbsPath, DirEntry};

use crate::error::{from_api, CmdResult};
use crate::state::AppState;

/// The daemon's requests carry paths as strings.
fn wire(path: &AbsPath) -> String {
    path.to_string()
}

#[tauri::command]
pub async fn vault_list(state: State<'_, AppState>) -> CmdResult<Vec<VaultSummary>> {
    let s = state.session().await?;
    let listing = s.client.call(ListRequest {}).await.map_err(from_api)?;
    Ok(listing
        .vaults
        .into_iter()
        // A vault that failed to load can't be opened; leave it out.
        .filter(|v| v.error.is_none())
        .map(|v| VaultSummary {
            id: v.vault_id,
            name: v.name.unwrap_or_default(),
        })
        .collect())
}

#[tauri::command]
pub async fn vault_create(
    state: State<'_, AppState>,
    args: CreateVault,
) -> CmdResult<VaultSummary> {
    let s = state.session().await?;
    let r = s
        .client
        .call(CreateRequest {
            name: args.name,
            private: false,
        })
        .await
        .map_err(from_api)?;
    Ok(VaultSummary {
        id: r.vault_id,
        name: r.name,
    })
}

#[tauri::command]
pub async fn vault_head(state: State<'_, AppState>, args: VaultRef) -> CmdResult<Head> {
    let s = state.session().await?;
    let r = s
        .client
        .call(HeadRequest {
            vault_id: args.vault,
        })
        .await
        .map_err(from_api)?;
    Ok(Head {
        id: r.vault_id,
        height: r.height,
        link: r.link,
    })
}

#[tauri::command]
pub async fn vault_ls(state: State<'_, AppState>, args: VaultPath) -> CmdResult<Vec<DirEntry>> {
    let s = state.session().await?;
    let r = s
        .client
        .call(LsRequest {
            vault_id: args.vault,
            path: wire(&args.path),
        })
        .await
        .map_err(from_api)?;
    Ok(r.items)
}

#[tauri::command]
pub async fn vault_cat(state: State<'_, AppState>, args: VaultPath) -> CmdResult<Response> {
    let s = state.session().await?;
    let r = s
        .client
        .call(CatRequest {
            vault_id: args.vault,
            path: wire(&args.path),
        })
        .await
        .map_err(from_api)?;
    Ok(Response::new(r.bytes.to_vec()))
}

/// Raw-body command: the file's bytes are the request body; the target
/// rides in [`AddHeaders`]. Returns the vault's new height.
#[tauri::command]
pub async fn vault_add(state: State<'_, AppState>, request: Request<'_>) -> CmdResult<u64> {
    let s = state.session().await?;
    let target = AddHeaders::from_lookup(|name| {
        request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    })?;
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(CommandError::BadRequest(
            "vault_add expects a raw byte body".into(),
        ));
    };
    let r = s
        .client
        .call(AddRequest {
            vault_id: target.vault,
            path: wire(&target.path),
            bytes: bytes.clone().into(),
        })
        .await
        .map_err(from_api)?;
    Ok(r.height)
}

#[tauri::command]
pub async fn vault_mkdir(state: State<'_, AppState>, args: VaultPath) -> CmdResult<u64> {
    let s = state.session().await?;
    let r = s
        .client
        .call(MkdirRequest {
            vault_id: args.vault,
            path: wire(&args.path),
            parents: false,
        })
        .await
        .map_err(from_api)?;
    Ok(r.height)
}

#[tauri::command]
pub async fn vault_rm(state: State<'_, AppState>, args: VaultPath) -> CmdResult<u64> {
    let s = state.session().await?;
    let r = s
        .client
        .call(RmRequest {
            vault_id: args.vault,
            path: wire(&args.path),
        })
        .await
        .map_err(from_api)?;
    Ok(r.height)
}

#[tauri::command]
pub async fn vault_mv(state: State<'_, AppState>, args: VaultMove) -> CmdResult<u64> {
    let s = state.session().await?;
    let r = s
        .client
        .call(MvRequest {
            vault_id: args.vault,
            from: wire(&args.from),
            to: wire(&args.to),
        })
        .await
        .map_err(from_api)?;
    Ok(r.height)
}
