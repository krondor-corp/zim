//! Hub login commands: the device-code flow, front-ended by the UI.
//! The verification URL + code go to the web view *and* the system
//! browser; `complete` then blocks until the user approves.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use zim::context::paths;
use zim::hub::login::{self, CompleteOptions};

use crate::state::AppState;

/// Matches the hub's grant TTL, like `zim hub login`.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Serialize)]
pub struct PairInfo {
    pub hub_url: String,
    pub verification_url: String,
    pub code: String,
    pub pubkey: String,
}

#[derive(Serialize)]
pub struct LoginInfo {
    pub hub_url: String,
}

/// The hub `zim hub login` would use by default.
#[tauri::command]
pub fn default_hub() -> &'static str {
    zim::cli::ops::hub::login::DEFAULT_HUB
}

#[tauri::command]
pub async fn hub_login_start(
    app: AppHandle,
    state: State<'_, AppState>,
    hub_url: String,
) -> Result<PairInfo, String> {
    let session = state.session().await?;
    let pending = login::start(&session.home, &hub_url, None)
        .await
        .map_err(|e| e.to_string())?;
    let info = PairInfo {
        hub_url: pending.hub_url().to_string(),
        verification_url: pending.verification_url().to_string(),
        code: pending.code().to_string(),
        pubkey: pending.pubkey_hex().to_string(),
    };
    // Best effort: the UI shows the URL too, in case no browser opens.
    let _ = app.opener().open_url(&info.verification_url, None::<&str>);
    *state.pending_login.lock().await = Some(pending);
    Ok(info)
}

#[tauri::command]
pub async fn hub_login_complete(state: State<'_, AppState>) -> Result<LoginInfo, String> {
    let pending = state
        .pending_login
        .lock()
        .await
        .take()
        .ok_or("no hub login in progress")?;
    let out = pending
        .complete(CompleteOptions {
            timeout: LOGIN_TIMEOUT,
            peer_nick: Some("hub".to_string()),
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(LoginInfo {
        hub_url: out.hub_url,
    })
}

#[tauri::command]
pub async fn hub_linked(state: State<'_, AppState>) -> Result<bool, String> {
    let session = state.session().await?;
    Ok(paths::hub_session_file(&session.home).exists())
}
