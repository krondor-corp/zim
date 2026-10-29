//! Hub login commands: the device-code flow, front-ended by the UI.
//! The verification URL + code go to the web view *and* the system
//! browser; `complete` then blocks until the user approves.

use std::time::Duration;

use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use zim::hub::login::{self, CompleteOptions};
use zim_api::desktop::{CommandError, LoginInfo, LoginStart, PairInfo};
use zim_api::Url;
use zim_crypto::PublicKey;

use crate::error::CmdResult;
use crate::state::AppState;

/// Matches the hub's grant TTL, like `zim hub login`.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(600);

fn login_err(e: impl std::fmt::Display) -> CommandError {
    CommandError::Login(e.to_string())
}

/// The hub `zim hub login` would use by default.
#[tauri::command]
pub fn default_hub() -> CmdResult<Url> {
    Url::parse(zim::cli::ops::hub::login::DEFAULT_HUB).map_err(login_err)
}

#[tauri::command]
pub async fn hub_login_start(
    app: AppHandle,
    state: State<'_, AppState>,
    args: LoginStart,
) -> CmdResult<PairInfo> {
    let session = state.session().await?;
    let pending = login::start(&session.home, args.hub_url.as_str(), None)
        .await
        .map_err(login_err)?;
    let info = PairInfo {
        hub_url: Url::parse(pending.hub_url()).map_err(login_err)?,
        verification_url: Url::parse(pending.verification_url()).map_err(login_err)?,
        code: pending.code().to_string(),
        pubkey: PublicKey::from_hex(pending.pubkey_hex()).map_err(login_err)?,
    };
    // Best effort: the UI shows the URL too, in case no browser opens.
    let _ = app
        .opener()
        .open_url(info.verification_url.as_str(), None::<&str>);
    *state.pending_login.lock().await = Some(pending);
    Ok(info)
}

#[tauri::command]
pub async fn hub_login_complete(state: State<'_, AppState>) -> CmdResult<LoginInfo> {
    let pending = state
        .pending_login
        .lock()
        .await
        .take()
        .ok_or(CommandError::NoLoginInProgress)?;
    let out = pending
        .complete(CompleteOptions {
            timeout: LOGIN_TIMEOUT,
            peer_nick: Some("hub".to_string()),
        })
        .await
        .map_err(login_err)?;
    Ok(LoginInfo {
        hub_url: Url::parse(&out.hub_url).map_err(login_err)?,
    })
}
