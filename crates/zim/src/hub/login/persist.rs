//! What a successful login leaves behind in `$ZIM_HOME`: the session
//! marker, and (optionally) the hub as a contact in the peer book.

use std::path::{Path, PathBuf};

use zim_api::hub::DidDocRequest;
use zim_api::Client;

use super::LoginError;
use crate::context::{paths, HubSession};

/// Write `$ZIM_HOME/hub-session.json` (mode 0600 on unix) and return its
/// path.
pub(super) async fn write_session(
    home: &Path,
    hub_url: &str,
    pubkey_hex: &str,
) -> Result<PathBuf, LoginError> {
    let session = HubSession {
        hub_url: hub_url.to_string(),
        enrolled_pubkey: pubkey_hex.to_string(),
        enrolled_at: chrono::Utc::now().to_rfc3339(),
    };
    let path = paths::hub_session_file(home);
    tokio::fs::write(&path, serde_json::to_string_pretty(&session)?).await?;
    #[cfg(unix)]
    {
        // Mode 0600 is overcautious now that the file carries no secrets,
        // but it keeps the contract that anything under $ZIM_HOME is
        // owner-only.
        use std::os::unix::fs::PermissionsExt;
        let _ = tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).await;
    }
    Ok(path)
}

/// Upsert the hub into the local peer book under `nick` as an
/// *untrusted* contact (a relay endpoint, never auto-shared into
/// vaults), and return the hub's DID.
///
/// The DID is read off the hub's own `/.well-known/did.json` rather than
/// derived from the URL the user typed: `--hub http://localhost:8080`
/// against a hub configured as `127.0.0.1:17190` would otherwise record
/// `did:web:localhost%3A8080`, which then fails to resolve because the
/// document self-identifies differently. The hub is the authority on its
/// own DID.
pub(super) async fn add_hub_to_peer_book(
    client: &Client,
    home: &Path,
    nick: &str,
) -> Result<String, LoginError> {
    let did = fetch_hub_did(client).await?;
    let identity = zim_did::Did::parse(&did).map_err(|e| LoginError::PeerBook(e.to_string()))?;
    let store = zim_peer::SqlitePeerStore::open(&paths::log_file(home))
        .map_err(|e| LoginError::PeerBook(e.to_string()))?;
    zim_peer::PeerStore::upsert(&store, nick, identity, false, Some("zim hub".to_string()))
        .await
        .map_err(|e| LoginError::PeerBook(e.to_string()))?;
    Ok(did)
}

async fn fetch_hub_did(client: &Client) -> Result<String, LoginError> {
    let path = "/.well-known/did.json";
    let url = client.base().join(path).expect("static path");
    let doc = client
        .call(DidDocRequest {
            path: path.to_string(),
        })
        .await
        .map_err(|e| LoginError::Hub(format!("fetch {url}: {e}")))?;
    if !doc.id.starts_with("did:web:") {
        return Err(LoginError::BadHubUrl(format!(
            "hub DID document `id` is not did:web: {}",
            doc.id
        )));
    }
    Ok(doc.id)
}
