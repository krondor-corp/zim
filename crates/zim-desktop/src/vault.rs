//! Vault commands: each proxies one daemon endpoint through the typed
//! client — the same requests the CLI sends. File bytes cross the IPC
//! boundary raw (`tauri::ipc::Response` out, request body in), never as
//! JSON number arrays.

use serde::Serialize;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::State;
use zim::daemon::api::v0::vault::add::AddRequest;
use zim::daemon::api::v0::vault::cat::CatRequest;
use zim::daemon::api::v0::vault::create::CreateRequest;
use zim::daemon::api::v0::vault::head::HeadRequest;
use zim::daemon::api::v0::vault::list::ListRequest;
use zim::daemon::api::v0::vault::ls::{EntryKind, LsRequest};
use zim::daemon::api::v0::vault::mkdir::MkdirRequest;
use zim::daemon::api::v0::vault::mv::MvRequest;
use zim::daemon::api::v0::vault::rm::RmRequest;
use zim_core::vault::VaultId;

use crate::state::AppState;

#[derive(Serialize)]
pub struct VaultItem {
    pub vault_id: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct Head {
    pub vault_id: String,
    pub height: u64,
}

#[derive(Serialize)]
pub struct Entry {
    pub name: String,
    /// `"file"` or `"dir"`.
    pub kind: &'static str,
}

fn vault_id(id: &str) -> Result<VaultId, String> {
    id.parse().map_err(|e| format!("bad vault id {id}: {e}"))
}

/// The daemon wants absolute paths; the CLI's `normalize_path` rule.
pub fn abs(path: &str) -> String {
    match path {
        "" | "." => "/".to_string(),
        p if p.starts_with('/') => p.to_string(),
        p => format!("/{p}"),
    }
}

#[tauri::command]
pub async fn vault_list(state: State<'_, AppState>) -> Result<Vec<VaultItem>, String> {
    let s = state.session().await?;
    let listing = s
        .client
        .call(ListRequest {})
        .await
        .map_err(|e| e.to_string())?;
    Ok(listing
        .vaults
        .into_iter()
        // A vault that failed to load can't be opened; leave it out.
        .filter(|v| v.error.is_none())
        .map(|v| VaultItem {
            vault_id: v.vault_id.to_string(),
            name: v.name.unwrap_or_default(),
        })
        .collect())
}

#[tauri::command]
pub async fn vault_create(state: State<'_, AppState>, name: String) -> Result<VaultItem, String> {
    let s = state.session().await?;
    let r = s
        .client
        .call(CreateRequest {
            name,
            private: false,
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(VaultItem {
        vault_id: r.vault_id.to_string(),
        name: r.name,
    })
}

#[tauri::command]
pub async fn vault_head(state: State<'_, AppState>, vault_id: String) -> Result<Head, String> {
    let s = state.session().await?;
    let r = s
        .client
        .call(HeadRequest {
            vault_id: self::vault_id(&vault_id)?,
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(Head {
        vault_id,
        height: r.height,
    })
}

#[tauri::command]
pub async fn vault_ls(
    state: State<'_, AppState>,
    vault_id: String,
    path: String,
) -> Result<Vec<Entry>, String> {
    let s = state.session().await?;
    let r = s
        .client
        .call(LsRequest {
            vault_id: self::vault_id(&vault_id)?,
            path: abs(&path),
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(r.items
        .into_iter()
        .map(|i| Entry {
            name: i.name,
            kind: match i.kind {
                EntryKind::File => "file",
                EntryKind::Dir => "dir",
            },
        })
        .collect())
}

#[tauri::command]
pub async fn vault_cat(
    state: State<'_, AppState>,
    vault_id: String,
    path: String,
) -> Result<Response, String> {
    let s = state.session().await?;
    let r = s
        .client
        .call(CatRequest {
            vault_id: self::vault_id(&vault_id)?,
            path: abs(&path),
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(Response::new(r.bytes.to_vec()))
}

/// Raw-body command: the file's bytes are the request body; the vault id
/// and (percent-encoded) path ride in headers. See `backend/desktop.rs`.
#[tauri::command]
pub async fn vault_add(state: State<'_, AppState>, request: Request<'_>) -> Result<u64, String> {
    let s = state.session().await?;
    let header = |name: &str| {
        request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
            .ok_or_else(|| format!("missing {name} header"))
    };
    let id = vault_id(&header("x-zim-vault")?)?;
    let path = percent_decode(&header("x-zim-path")?)?;
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err("vault_add expects a raw byte body".to_string());
    };
    let r = s
        .client
        .call(AddRequest {
            vault_id: id,
            path: abs(&path),
            bytes: bytes.clone().into(),
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(r.height)
}

#[tauri::command]
pub async fn vault_mkdir(
    state: State<'_, AppState>,
    vault_id: String,
    path: String,
) -> Result<u64, String> {
    let s = state.session().await?;
    let r = s
        .client
        .call(MkdirRequest {
            vault_id: self::vault_id(&vault_id)?,
            path: abs(&path),
            parents: false,
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(r.height)
}

#[tauri::command]
pub async fn vault_rm(
    state: State<'_, AppState>,
    vault_id: String,
    path: String,
) -> Result<u64, String> {
    let s = state.session().await?;
    let r = s
        .client
        .call(RmRequest {
            vault_id: self::vault_id(&vault_id)?,
            path: abs(&path),
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(r.height)
}

#[tauri::command]
pub async fn vault_mv(
    state: State<'_, AppState>,
    vault_id: String,
    from: String,
    to: String,
) -> Result<u64, String> {
    let s = state.session().await?;
    let r = s
        .client
        .call(MvRequest {
            vault_id: self::vault_id(&vault_id)?,
            from: abs(&from),
            to: abs(&to),
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(r.height)
}

/// Undo JS `encodeURIComponent` (headers can't carry raw UTF-8 paths).
fn percent_decode(s: &str) -> Result<String, String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3).ok_or("truncated %-escape in path")?;
            out.push(u8::from_str_radix(hex, 16).map_err(|_| "bad %-escape in path")?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "path is not UTF-8".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alice_notes_with_spaces_and_unicode_survive_the_header_round_trip() {
        // JS sends encodeURIComponent("/Notes/café plan.md").
        let sent = "%2FNotes%2Fcaf%C3%A9%20plan.md";
        assert_eq!(percent_decode(sent).unwrap(), "/Notes/café plan.md");
    }

    #[test]
    fn bob_gets_an_error_for_a_truncated_escape() {
        assert!(percent_decode("%2").is_err());
        assert!(percent_decode("%zz").is_err());
    }

    #[test]
    fn paths_are_made_absolute_like_the_cli_does() {
        assert_eq!(abs(""), "/");
        assert_eq!(abs("."), "/");
        assert_eq!(abs("notes/a.md"), "/notes/a.md");
        assert_eq!(abs("/notes/a.md"), "/notes/a.md");
    }
}
