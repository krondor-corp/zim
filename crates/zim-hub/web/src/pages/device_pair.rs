//! Device pairing — the page the daemon's `zim hub login` sends you to. Reads
//! `?code=` from the URL, shows the pending grant (label + pubkey), and an
//! Approve button → the hub stamps the grant and the daemon enrolls itself.

use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::api::{approve_grant, fetch_grant, GrantInfo};

/// Pull `code` out of `?code=ABCD-EFGH`.
fn query_code() -> String {
    web_sys::window()
        .and_then(|w| w.location().search().ok())
        .map(|s| {
            s.trim_start_matches('?')
                .split('&')
                .find_map(|kv| kv.strip_prefix("code="))
                .unwrap_or_default()
                .to_string()
        })
        .unwrap_or_default()
}

#[function_component(DevicePair)]
pub fn device_pair() -> Html {
    let code = use_state(query_code);
    let grant = use_state(|| None::<GrantInfo>);
    let error = use_state(String::new);
    let approved = use_state(|| false);

    {
        let grant = grant.clone();
        let error = error.clone();
        use_effect_with((*code).clone(), move |code| {
            let code = code.clone();
            if !code.is_empty() {
                spawn_local(async move {
                    match fetch_grant(&code).await {
                        Ok(g) => grant.set(Some(g)),
                        Err(e) => error.set(e),
                    }
                });
            }
            || ()
        });
    }

    let on_approve = {
        let code = (*code).clone();
        let approved = approved.clone();
        let error = error.clone();
        Callback::from(move |_: MouseEvent| {
            let code = code.clone();
            let approved = approved.clone();
            let error = error.clone();
            spawn_local(async move {
                match approve_grant(&code).await {
                    Ok(()) => approved.set(true),
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let body = if *approved {
        html! {
            <section class="settings-card"><div class="settings-card__body">
                <p>{ "Approved \u{2705} \u{2014} your daemon should finish enrolling within a few seconds." }</p>
            </div></section>
        }
    } else if (*code).is_empty() {
        html! { <p class="page-lead">{ "Open the link your daemon printed (it ends with ?code=…)." }</p> }
    } else {
        match &*grant {
            None => html! { <p class="muted">{ "Loading\u{2026}" }</p> },
            Some(g) if g.status == "pending" => html! {
                <section class="settings-card">
                    <div class="settings-card__head">
                        <h2>{ &g.label }</h2>
                        <p>{ "A device wants to enroll with your account. Verify the key, then approve." }</p>
                    </div>
                    <div class="settings-card__body">
                        <div class="field"><label>{ "Code" }</label>
                            <input type="text" value={(*code).clone()} readonly=true class="mono" /></div>
                        <div class="field"><label>{ "Public key" }</label>
                            <input type="text" value={g.pubkey.clone()} readonly=true class="mono" /></div>
                    </div>
                    <div class="settings-card__foot">
                        <button type="button" class="btn btn-primary" onclick={on_approve}>{ "Approve" }</button>
                    </div>
                </section>
            },
            Some(g) => html! {
                <p class="page-lead">{ match g.status.as_str() {
                    "approved" => "This code was already approved.",
                    "expired" => "This code expired \u{2014} run `zim hub login` again.",
                    _ => "Code not found. Check for typos.",
                } }</p>
            },
        }
    };

    html! {
        <div class="onb"><main class="onb__main"><div class="onb__inner">
            <span class="page-eyebrow">{ "device pairing" }</span>
            <h1>{ "Approve a device" }</h1>
            if !(*error).is_empty() {
                <div class="error">{ (*error).clone() }</div>
            }
            { body }
        </div></main></div>
    }
}
