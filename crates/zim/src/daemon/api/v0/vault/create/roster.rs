//! The account's device roster, as genesis shares.
//!
//! Same resolution `zim hub peers sync` does — `GET /api/v0/devices` on
//! the paired hub — but sealed straight into a new vault instead of
//! folded into the address book. Daemons are dialed directly; web keys
//! have no iroh endpoint, so they're reached *via* the hub's key.

use zim_api::hub::{Device, DeviceKind, HubClient};
use zim_crypto::PublicKey;
use zim_did::Did;

use crate::context::hub_session::HubSessionReadError;
use crate::context::HubSession;
use crate::ServiceState;

/// One genesis share: the device key and, for a web key, the hub it's
/// reached through.
pub type DeviceShare = (PublicKey, Option<Did>);

#[derive(Debug, thiserror::Error)]
pub enum RosterError {
    #[error(transparent)]
    Session(#[from] HubSessionReadError),
    /// The hub couldn't be asked. Surfaced, not swallowed: silently
    /// falling back to owner-only is exactly the bug this replaces.
    #[error("share with your devices: {0} (use --private to keep the vault on this device only)")]
    Hub(String),
}

/// Every device on the account this daemon is paired with, or nothing
/// when it isn't paired.
pub async fn account_devices(state: &ServiceState) -> Result<Vec<DeviceShare>, RosterError> {
    let Some(session) = HubSession::load(state.home())? else {
        return Ok(Vec::new());
    };
    let hub = |e: &dyn std::fmt::Display| RosterError::Hub(e.to_string());
    let client =
        HubClient::new(&session.hub_url, state.peer().secret().clone()).map_err(|e| hub(&e))?;
    let devices = client.devices().await.map_err(|e| hub(&e))?;

    // Only resolve the hub's own key when a web device needs it as `via`.
    let hub_key = if devices.iter().any(is_web) {
        let doc = client
            .did_doc("/.well-known/did.json")
            .await
            .map_err(|e| hub(&e))?;
        let did = Did::parse(&doc.id).map_err(|e| hub(&e))?;
        let key = zim_did::resolve_pubkey(&did, state.resolver().as_ref())
            .await
            .map_err(|e| hub(&e))?;
        Some(key)
    } else {
        None
    };
    Ok(device_shares(&devices, hub_key))
}

fn is_web(d: &Device) -> bool {
    d.kind == DeviceKind::Web
}

/// Map roster rows to genesis shares. A web device is reached via
/// `hub_key`.
fn device_shares(devices: &[Device], hub_key: Option<PublicKey>) -> Vec<DeviceShare> {
    let via = hub_key.map(|k| Did::from_key(&k));
    devices
        .iter()
        .map(|d| {
            let reach = if is_web(d) { via.clone() } else { None };
            (d.pubkey, reach)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use zim_crypto::PrivateKey;

    use super::*;

    fn device(key: &PublicKey, kind: DeviceKind) -> Device {
        Device {
            pubkey: *key,
            did: None,
            label: None,
            kind,
            created_at: None,
        }
    }

    #[test]
    fn alices_daemons_are_dialed_directly_and_her_browser_through_the_hub() {
        let laptop = PrivateKey::generate().public();
        let desktop = PrivateKey::generate().public();
        let browser = PrivateKey::generate().public();
        let hub = PrivateKey::generate().public();

        // Alice's account roster: two daemons and a browser key.
        let roster = [
            device(&laptop, DeviceKind::Daemon),
            device(&desktop, DeviceKind::Daemon),
            device(&browser, DeviceKind::Web),
        ];

        let shares = device_shares(&roster, Some(hub));

        assert_eq!(
            shares,
            vec![
                (laptop, None),
                (desktop, None),
                (browser, Some(Did::from_key(&hub))),
            ]
        );
    }
}
