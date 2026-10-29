//! Typed fetch layer for the cookie-authed JSON endpoints.
//!
//! Every call goes through `zim_api`'s [`Client`] — the same `ApiRequest`
//! types the hub server mirrors and the `zim` CLI executes — via reqwest's
//! wasm/fetch backend. Same-origin cookies ride along by default (fetch's
//! `same-origin` credentials mode), so auth is untouched. The crypto/JWT
//! routes still go through the SDK (`HubClient`/`WasmFs`).
//!
//! Wire types live in `zim_api::hub` (one definition for the server, the
//! CLI, and this SPA).

use zim_api::{ApiError, ApiRequest, Client, Url};

// One definition per endpoint — server serializes, CLI + SPA deserialize.
pub use zim_api::hub::{
    AdminActionRequest, AdminUser, AdminUsers, AdminUsersRequest, Device, DevicesRequest, Me,
    MeRequest, RemoveDeviceRequest, VaultItem, VaultsRequest,
};

/// A [`Client`] rooted at this page's origin. Cheap to build per call —
/// reqwest's wasm client is a thin handle over the browser's `fetch`.
fn client() -> Result<Client, String> {
    let origin = web_sys::window()
        .ok_or("no window")?
        .location()
        .origin()
        .map_err(|_| "no origin".to_string())?;
    let base = Url::parse(&origin).map_err(|e| e.to_string())?;
    Client::new(&base).map_err(|e| e.to_string())
}

async fn call<R: ApiRequest>(req: R) -> Result<R::Response, String> {
    client()?.call(req).await.map_err(|e| e.to_string())
}

/// `Ok(None)` means unauthenticated (401) — caller should send to login.
pub async fn fetch_me() -> Result<Option<Me>, String> {
    match client()?.call(MeRequest).await {
        Ok(me) => Ok(Some(me)),
        Err(ApiError::HttpStatus(status, _)) if status.as_u16() == 401 => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub async fn fetch_devices() -> Result<Vec<Device>, String> {
    Ok(call(DevicesRequest).await?.devices)
}

pub async fn delete_device(pubkey: &str) -> Result<(), String> {
    call(RemoveDeviceRequest {
        pubkey: pubkey.to_string(),
    })
    .await
}

pub async fn fetch_vaults() -> Result<Vec<VaultItem>, String> {
    Ok(call(VaultsRequest).await?.vaults)
}

// ─── Admin (RequireAdmin) ─────────────────────────────────────────────

pub async fn fetch_admin_users() -> Result<AdminUsers, String> {
    call(AdminUsersRequest).await
}

/// `action` is one of `authorize` / `unauthorize` / `promote` / `demote`.
pub async fn admin_action(user_id: &str, action: &str) -> Result<(), String> {
    call(AdminActionRequest {
        user_id: user_id.to_string(),
        action: action.to_string(),
    })
    .await
}
