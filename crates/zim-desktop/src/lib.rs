//! zim desktop: a Tauri shell around the hub SPA, backed by the local
//! daemon's HTTP API (the same one the CLI talks to).
//!
//! - [`connect`] — connect-or-start decision logic (pure, tested)
//! - [`sidecar`] — the real daemon host: lock probe, detached `zim daemon run`
//! - [`boot`] — first-run init + connect, returns what the UI needs
//! - [`login`] — hub device-code login commands
//! - [`vault`] — vault commands proxying the daemon's typed client

mod boot;
mod connect;
mod error;
mod login;
mod sidecar;
mod state;
mod vault;

use tauri::State;
use zim_api::desktop::BootInfo;

use crate::error::CmdResult;
use crate::state::AppState;

#[tauri::command]
async fn boot(state: State<'_, AppState>) -> CmdResult<BootInfo> {
    boot::boot(&state).await
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            boot,
            login::default_hub,
            login::hub_login_start,
            login::hub_login_complete,
            vault::vault_list,
            vault::vault_create,
            vault::vault_head,
            vault::vault_ls,
            vault::vault_cat,
            vault::vault_add,
            vault::vault_mkdir,
            vault::vault_rm,
            vault::vault_mv,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the zim desktop app");
}
