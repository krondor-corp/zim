//! Launch sequence: init on first run, connect-or-start the daemon.

use std::path::Path;

use url::Url;
use zim::cli::ops::init::init_home;
use zim::context::paths;
use zim::daemon::api::client::ApiClient;
use zim_api::desktop::{BootInfo, CommandError};
use zim_crypto::PublicKey;

use crate::connect::{connect_or_start, Connection, Patience};
use crate::error::CmdResult;
use crate::sidecar::{default_binary, SidecarHost};
use crate::state::{AppState, Session};

/// Idempotent: safe to call again after a reload of the web view.
pub async fn boot(state: &AppState) -> CmdResult<BootInfo> {
    let boot = |e: &dyn std::fmt::Display| CommandError::Boot(e.to_string());
    let home = paths::home_dir(None).map_err(|e| boot(&e))?;
    let init = init_home(&home).await.map_err(|e| boot(&e))?;
    let peer_id = PublicKey::from_hex(&init.identity_hex).map_err(|e| boot(&e))?;

    let host = SidecarHost::new(home.clone(), default_binary());
    let conn = connect_or_start(&host, Patience::default())
        .await
        .map_err(|e| boot(&e))?;

    let endpoint =
        Url::parse(&format!("http://127.0.0.1:{}", conn.port())).map_err(|e| boot(&e))?;
    let client = ApiClient::new(&endpoint).map_err(|e| boot(&e))?;
    state
        .set_session(Session {
            home: home.clone(),
            client,
        })
        .await;

    Ok(BootInfo {
        hub_linked: hub_linked(&home),
        home,
        peer_id,
        port: conn.port(),
        first_run: init.identity_was_new,
        started_daemon: matches!(conn, Connection::Started { .. }),
    })
}

fn hub_linked(home: &Path) -> bool {
    paths::hub_session_file(home).exists()
}
