//! Devices view: the keys enrolled to the account — a debug surface for
//! "which keys exist, and what do new vaults share to?". Every key here is
//! a `did:web` verification method; when you create a vault in the browser
//! it's shared to *all* of them. The web key is the master identity (created
//! via the gate, not deletable here — deleting it locks you out); daemons
//! enroll via `zim hub login` and can be removed.

use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::api::{delete_device, fetch_devices, fetch_me, Device};
use crate::components::copy_button;
use crate::components::dialog::ConfirmDialog;

#[function_component(Devices)]
pub fn devices() -> Html {
    let list = use_state(|| None::<Vec<Device>>);
    let error = use_state(String::new);
    // The account's `did:web` — what we show for the web key. Fetched once;
    // empty until it arrives (then the web row falls back to its `did:key`).
    let web_did = use_state(String::new);

    let reload = {
        let list = list.clone();
        let error = error.clone();
        Callback::from(move |_: ()| {
            let list = list.clone();
            let error = error.clone();
            spawn_local(async move {
                match fetch_devices().await {
                    Ok(d) => list.set(Some(d)),
                    Err(e) => error.set(e),
                }
            });
        })
    };

    {
        let reload = reload.clone();
        let web_did = web_did.clone();
        use_effect_with((), move |_| {
            reload.emit(());
            // The account DID lives on `/me`, not the device roster.
            spawn_local(async move {
                if let Ok(Some(me)) = fetch_me().await {
                    web_did.set(me.did);
                }
            });
            || ()
        });
    }

    // Which device the user is confirming removal of.
    let pending = use_state(|| None::<String>);
    let on_delete = {
        let pending = pending.clone();
        Callback::from(move |pubkey: String| pending.set(Some(pubkey)))
    };
    let confirm_delete = {
        let reload = reload.clone();
        let error = error.clone();
        let pending = pending.clone();
        Callback::from(move |_: ()| {
            let Some(pubkey) = (*pending).clone() else { return; };
            pending.set(None);
            let reload = reload.clone();
            let error = error.clone();
            spawn_local(async move {
                match delete_device(&pubkey).await {
                    Ok(()) => reload.emit(()),
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let count = match &*list {
        Some(d) => d.len(),
        None => 0,
    };

    html! {
        <>
            <span class="page-eyebrow">{ "devices" }</span>
            <h1>{ "Devices" }</h1>
            <p class="page-lead">
                { "Keys enrolled to your account. A new vault you create in the browser is \
                   shared to " }<strong>{ "every key listed here" }</strong>{ " \u{2014} so a daemon \
                   only receives shares for vaults created " }<em>{ "after" }</em>{ " it enrolls via " }
                <code>{ "zim hub login" }</code>{ "." }
            </p>
            if !(*error).is_empty() {
                <div class="error">{ (*error).clone() }</div>
            }
            <section class="settings-card">
                <div class="settings-card__head">
                    <h2>{ "Enrolled keys" }</h2>
                    <p>
                        {
                            match &*list {
                                None => "Loading\u{2026}".to_string(),
                                Some(_) => format!(
                                    "{count} key{} \u{2014} the verification methods in your DID document.",
                                    if count == 1 { "" } else { "s" },
                                ),
                            }
                        }
                    </p>
                </div>
                {
                    match &*list {
                        None => html! { <div class="settings-card__body"><p class="muted">{ "Loading\u{2026}" }</p></div> },
                        Some(devs) if devs.is_empty() => html! {
                            <div class="settings-card__body"><div class="section-empty">{ "No devices enrolled." }</div></div>
                        },
                        Some(devs) => html! {
                            <div class="device-list">
                                { for devs.iter().map(|d| device_row(d, &web_did, &on_delete)) }
                            </div>
                        },
                    }
                }
            </section>
            if pending.is_some() {
                <ConfirmDialog title="Remove device?"
                    body="This key can no longer decrypt vaults shared with it. Vaults it authored keep their history."
                    action="Remove"
                    on_cancel={let p = pending.clone(); Callback::from(move |_: ()| p.set(None))}
                    on_confirm={confirm_delete} />
            }
        </>
    }
}

fn device_row(d: &Device, web_did: &str, on_delete: &Callback<String>) -> Html {
    let is_web = d.kind == "web";
    let label = d.label.clone().unwrap_or_else(|| {
        if is_web {
            "web key".to_string()
        } else {
            "daemon".to_string()
        }
    });
    let enrolled = d.created_at.get(..10).unwrap_or(&d.created_at).to_string();
    let pubkey = d.pubkey.clone();
    // The web key's proper identity is the account `did:web` (reached via the
    // hub) — that's what you share with, not its raw `did:key`. Daemons are
    // dialed directly, so they show their `did:key` (or hex on an older hub).
    let did = if is_web && !web_did.is_empty() {
        web_did.to_string()
    } else if d.did.is_empty() {
        d.pubkey.clone()
    } else {
        d.did.clone()
    };
    let on_del = {
        let pubkey = pubkey.clone();
        let on_delete = on_delete.clone();
        Callback::from(move |_: MouseEvent| on_delete.emit(pubkey.clone()))
    };

    html! {
        <div class="device-row">
            <span class="device-row__icon">{ if is_web { "\u{1F511}" } else { "\u{1F5A5}\u{FE0F}" } }</span>
            <div class="device-row__main">
                <div class="device-row__title">
                    <span class="device-row__label">{ label }</span>
                    <span class="device-row__badge">{ d.kind.clone() }</span>
                    if is_web {
                        <span class="device-row__badge device-row__badge--master">{ "master" }</span>
                    }
                </div>
                <div class="device-row__key">
                    <code>{ did.clone() }</code>
                    { copy_button(&did) }
                </div>
                <div class="device-row__meta">{ format!("enrolled {enrolled}") }</div>
            </div>
            if !is_web {
                <button type="button" class="btn btn-danger device-row__remove" onclick={on_del}>
                    { "Remove" }
                </button>
            }
        </div>
    }
}
