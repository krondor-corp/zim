//! Launch sequence: init on first run, connect-or-start the daemon.

use std::path::Path;

use serde::Serialize;
use zim::cli::ops::init::init_home;
use zim::context::paths;

use crate::connect::{connect_or_start, Connection, Patience};
use crate::sidecar::{default_binary, SidecarHost};
use crate::state::{AppState, Session};

#[derive(Debug, Serialize)]
pub struct BootInfo {
    pub home: String,
    /// This device's identity pubkey (hex) — the same one the CLI shows.
    pub peer_id: String,
    pub port: u16,
    /// The identity was minted by this launch.
    pub first_run: bool,
    /// We started the daemon (vs. attaching to a running one).
    pub started_daemon: bool,
    /// A hub session is already on disk.
    pub hub_linked: bool,
}

/// Idempotent: safe to call again after a reload of the web view.
pub async fn boot(state: &AppState) -> Result<BootInfo, String> {
    let home = paths::home_dir(None).map_err(|e| e.to_string())?;
    let init = init_home(&home).await.map_err(|e| e.to_string())?;

    let host = SidecarHost::new(home.clone(), default_binary());
    let conn = connect_or_start(&host, Patience::default())
        .await
        .map_err(|e| e.to_string())?;

    let endpoint =
        url::Url::parse(&format!("http://127.0.0.1:{}", conn.port())).map_err(|e| e.to_string())?;
    let client = zim::daemon::api::client::ApiClient::new(&endpoint).map_err(|e| e.to_string())?;
    state
        .set_session(Session {
            home: home.clone(),
            client,
        })
        .await;

    Ok(BootInfo {
        home: home.display().to_string(),
        peer_id: init.identity_hex,
        port: conn.port(),
        first_run: init.identity_was_new,
        started_daemon: matches!(conn, Connection::Started { .. }),
        hub_linked: hub_linked(&home),
    })
}

fn hub_linked(home: &Path) -> bool {
    paths::hub_session_file(home).exists()
}
