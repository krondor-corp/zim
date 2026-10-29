//! The vault Details panel: manifest metadata + shareholders, flagging
//! which keys are the signed-in user's own enrolled devices.

use yew::prelude::*;

use zim_crypto::PublicKey;
use zim_wasm::{ShareInfo, VaultInfo};

use crate::api::{Device, DeviceKind};

pub(crate) fn details_panel(meta: &VaultInfo, devices: &[Device], web_did: &str) -> Html {
    html! {
        <div class="card vault-details" style="padding:1rem;">
            <div style="display:grid;grid-template-columns:auto 1fr;gap:0.35rem 1rem;align-items:baseline;">
                <span class="muted">{ "name" }</span>
                <span>{ meta.name.clone() }</span>
                <span class="muted">{ "height" }</span>
                <span>{ meta.height }</span>
                <span class="muted">{ "vault id" }</span>
                <code style="word-break:break-all;font-size:0.72rem;">{ meta.id.to_string() }</code>
                <span class="muted">{ "manifest" }</span>
                <code style="word-break:break-all;font-size:0.72rem;">{ meta.manifest.to_string() }</code>
            </div>
            <h3 style="font-size:0.9rem;margin:1rem 0 0.5rem;">
                { format!("Shareholders ({})", meta.shares.len()) }
            </h3>
            <div style="display:flex;flex-direction:column;gap:0.4rem;">
                { for meta.shares.iter().map(|s| share_row(s, devices, web_did, &meta.author)) }
            </div>
        </div>
    }
}

fn share_row(s: &ShareInfo, devices: &[Device], web_did: &str, author: &PublicKey) -> Html {
    let mine = device_for(devices, &s.key);
    let is_author = s.key == *author;
    // The web key's proper identity is the account `did:web` (reached via the
    // hub), not the `did:key` sealed into the manifest. Only swap when we can
    // confirm this share *is* our web key (its pubkey matches the web device).
    let is_web_key = mine.is_some_and(|d| d.kind == DeviceKind::Web);
    let did = if is_web_key && !web_did.is_empty() {
        web_did.to_string()
    } else {
        s.identity.to_string()
    };
    html! {
        <div style="display:flex;align-items:center;gap:0.5rem;padding:0.5rem 0.6rem;border:1px solid hsl(var(--border));border-radius:var(--radius);">
            <code style="font-size:0.72rem;word-break:break-all;">{ did }</code>
            <span style="flex:1;"></span>
            if is_author {
                <span class="device-row__badge">{ "author" }</span>
            }
            if let Some(d) = mine {
                <span class="device-row__badge device-row__badge--master">{ own_label(d) }</span>
            } else {
                <span class="device-row__badge">{ "external" }</span>
            }
        </div>
    }
}

/// The account device holding `key`, if it's one of ours.
pub(crate) fn device_for<'a>(devices: &'a [Device], key: &PublicKey) -> Option<&'a Device> {
    devices.iter().find(|d| d.pubkey == *key)
}

pub(crate) fn own_label(d: &Device) -> String {
    match &d.label {
        Some(l) if !l.is_empty() => l.clone(),
        _ if d.kind == DeviceKind::Web => "your web key".to_string(),
        _ => "your daemon".to_string(),
    }
}
