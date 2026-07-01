//! Thin fetch layer for the cookie-authed JSON endpoints (`/api/v0/me`,
//! `/api/v0/vaults`). The crypto/JWT routes go through the SDK
//! (`HubClient`/`WasmFs`); these are plain same-origin GETs, so gloo-net is
//! enough — cookies ride along by default.

use serde::Deserialize;

#[derive(Clone, PartialEq, Deserialize)]
pub struct Me {
    pub user_id: String,
    pub host: String,
    pub email: String,
    /// The account's `did:web:<host>:u:<user_id>` — the web key's proper
    /// identity (reached via the hub). Shown/copied for the web-key row on
    /// the devices page instead of its raw `did:key`. `#[serde(default)]`
    /// tolerates an older hub that doesn't send it.
    #[serde(default)]
    pub did: String,
    /// Whether this account has a web key enrolled on the hub. Drives the
    /// gate's create-vs-unlock choice; `#[serde(default)]` keeps it
    /// tolerant of an older hub that doesn't send the field.
    #[serde(default)]
    pub has_web_key: bool,
}

#[derive(Clone, PartialEq, Deserialize)]
pub struct VaultItem {
    pub vault_id: String,
    pub name: String,
}

#[derive(Deserialize)]
struct VaultsResponse {
    vaults: Vec<VaultItem>,
}

/// One entry as `WasmFs::ls` serializes it.
#[derive(Clone, PartialEq, Deserialize)]
pub struct FsEntry {
    pub name: String,
    pub kind: String, // "file" | "dir"
    pub hash: String,
    pub mime: Option<String>,
}

/// `Ok(None)` means unauthenticated (401) — caller should send to login.
pub async fn fetch_me() -> Result<Option<Me>, String> {
    let resp = gloo_net::http::Request::get("/api/v0/me")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status() == 401 {
        return Ok(None);
    }
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<Me>().await.map(Some).map_err(|e| e.to_string())
}

// Shared wire types — same `Device` the hub server serializes and the
// `zim` CLI reads (`zim_api::hub`). No reqwest is pulled (types-only dep).
pub use zim_api::hub::{Device, DevicesResponse};

pub async fn fetch_devices() -> Result<Vec<Device>, String> {
    let resp = gloo_net::http::Request::get("/api/v0/devices")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(resp
        .json::<DevicesResponse>()
        .await
        .map_err(|e| e.to_string())?
        .devices)
}

pub async fn delete_device(pubkey: &str) -> Result<(), String> {
    let resp = gloo_net::http::Request::delete(&format!("/api/v0/devices/{pubkey}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

pub async fn fetch_vaults() -> Result<Vec<VaultItem>, String> {
    let resp = gloo_net::http::Request::get("/api/v0/vaults")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(resp
        .json::<VaultsResponse>()
        .await
        .map_err(|e| e.to_string())?
        .vaults)
}

// ─── Admin (RequireAdmin) ─────────────────────────────────────────────

#[derive(Clone, PartialEq, Deserialize)]
pub struct AdminUser {
    pub id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub is_admin: bool,
    pub is_authorized: bool,
}

#[derive(Clone, PartialEq, Deserialize)]
pub struct AdminUsers {
    pub current_admin_id: String,
    pub users: Vec<AdminUser>,
}

pub async fn fetch_admin_users() -> Result<AdminUsers, String> {
    let resp = gloo_net::http::Request::get("/api/v0/admin/users")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<AdminUsers>().await.map_err(|e| e.to_string())
}

/// `action` is one of `authorize` / `unauthorize` / `promote` / `demote`.
pub async fn admin_action(user_id: &str, action: &str) -> Result<(), String> {
    let resp = gloo_net::http::Request::post(&format!("/api/v0/admin/users/{user_id}/{action}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}

// ─── Device-code approval (RequireUser) ───────────────────────────────

#[derive(Clone, PartialEq, Deserialize)]
pub struct GrantInfo {
    /// "pending" | "approved" | "expired" | "not_found"
    pub status: String,
    pub label: String,
    pub pubkey: String,
}

pub async fn fetch_grant(code: &str) -> Result<GrantInfo, String> {
    let resp = gloo_net::http::Request::get(&format!("/api/v0/auth/device-code/{code}"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<GrantInfo>().await.map_err(|e| e.to_string())
}

pub async fn approve_grant(code: &str) -> Result<(), String> {
    let resp = gloo_net::http::Request::post(&format!("/api/v0/auth/device-code/{code}/approve"))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(())
}
