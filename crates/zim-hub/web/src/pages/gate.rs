//! Blocking web-key gate: load the account's web key into WASM memory.
//! Tries the tab-cached seed, then unlock-from-escrow; if the account has no
//! web key yet, flips to a create form (enroll). No redirect — it's a step
//! inside the authenticated app.

use gloo_storage::{SessionStorage, Storage};
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use zim_wasm::HubClient;

use crate::api::Me;
use crate::util::{jserr, origin, rand_suffix, SS_KEY};

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Unlock,
    Create,
}

#[derive(Properties, PartialEq)]
pub struct Props {
    pub me: Me,
    pub on_ready: Callback<()>,
}

#[function_component(KeyGate)]
pub fn key_gate(props: &Props) -> Html {
    let has_web_key = props.me.has_web_key;
    // No web key on the hub yet → onboard (create); otherwise unlock.
    let mode = use_state(move || {
        if has_web_key {
            Mode::Unlock
        } else {
            Mode::Create
        }
    });
    let pass = use_state(String::new);
    let confirm = use_state(String::new);
    let error = use_state(String::new);
    let busy = use_state(|| false);

    {
        // Fast path: a seed cached for this tab → load it and we're done.
        // Only trust it when the hub actually has a web key for this
        // account — after a hub reset (or a different account) the cached
        // seed is stale and must NOT short-circuit onboarding.
        let on_ready = props.on_ready.clone();
        use_effect_with((), move |_| {
            if has_web_key {
                if let Ok(seed) = SessionStorage::get::<String>(SS_KEY) {
                    if let Ok(bytes) = hex::decode(&seed) {
                        if zim_wasm::load_key_from_session(&bytes).is_ok() {
                            on_ready.emit(());
                        }
                    }
                }
            }
            || ()
        });
    }

    let on_pass = {
        let pass = pass.clone();
        Callback::from(move |e: InputEvent| {
            let i: HtmlInputElement = e.target_unchecked_into();
            pass.set(i.value());
        })
    };
    let on_confirm = {
        let confirm = confirm.clone();
        Callback::from(move |e: InputEvent| {
            let i: HtmlInputElement = e.target_unchecked_into();
            confirm.set(i.value());
        })
    };

    let submit = {
        let mode = mode.clone();
        let pass = pass.clone();
        let confirm = confirm.clone();
        let error = error.clone();
        let busy = busy.clone();
        let on_ready = props.on_ready.clone();
        let me = props.me.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            let cur = *mode;
            let pp = (*pass).clone();
            if pp.len() < 8 {
                error.set("Passphrase must be at least 8 characters.".into());
                return;
            }
            if cur == Mode::Create && pp != *confirm {
                error.set("Passphrases do not match.".into());
                return;
            }
            let mode = mode.clone();
            let error = error.clone();
            let busy = busy.clone();
            let on_ready = on_ready.clone();
            let me = me.clone();
            busy.set(true);
            error.set(String::new());
            spawn_local(async move {
                let res = match cur {
                    Mode::Unlock => unlock(pp).await,
                    Mode::Create => create(&me, pp).await,
                };
                busy.set(false);
                match res {
                    Ok(()) => on_ready.emit(()),
                    Err(msg) if cur == Mode::Unlock && msg.contains("no escrowed key") => {
                        mode.set(Mode::Create); // never set up → create it
                    }
                    Err(msg) => error.set(msg),
                }
            });
        })
    };

    let creating = *mode == Mode::Create;
    html! {
        <div class="gate" style="max-width:440px;margin:0 auto;">
            <h3 style="margin-top:0;">
                { if creating { "Create your web key" } else { "Unlock your account" } }
            </h3>
            <p style="font-size:0.85rem;color:hsl(var(--muted-foreground));">
                { if creating {
                    "Your account needs a web key \u{2014} the master identity that controls your vaults. Choose a passphrase; it's encrypted into escrow and unlockable from any browser."
                } else {
                    "Enter your passphrase to unlock your web key \u{2014} your account's master identity. It's recovered from escrow and unwrapped locally."
                } }
            </p>
            <form onsubmit={submit}>
                <div class="field">
                    <label>{ "Passphrase" }</label>
                    <input type="password" oninput={on_pass} value={(*pass).clone()} />
                </div>
                if creating {
                    <div class="field">
                        <label>{ "Confirm passphrase" }</label>
                        <input type="password" oninput={on_confirm} value={(*confirm).clone()} />
                    </div>
                }
                if !(*error).is_empty() {
                    <div class="error" style="margin-bottom:0.75rem;">{ (*error).clone() }</div>
                }
                <button type="submit" class="btn btn-primary" disabled={*busy} style="width:100%;">
                    { if *busy { "Working\u{2026}" } else if creating { "Create web key" } else { "Unlock" } }
                </button>
            </form>
        </div>
    }
}

async fn unlock(pp: String) -> Result<(), String> {
    let client = HubClient::new(origin()).map_err(|e| jserr(e.into()))?;
    let key = client
        .unlock_from_escrow(pp)
        .await
        .map_err(|e| jserr(e.into()))?;
    SessionStorage::set(SS_KEY, key.seed_hex()).ok();
    Ok(())
}

async fn create(me: &Me, pp: String) -> Result<(), String> {
    let did = format!(
        "did:web:{}:u:{}#browser-{}",
        me.host,
        me.user_id,
        rand_suffix()
    );
    let client = HubClient::new(origin()).map_err(|e| jserr(e.into()))?;
    let key = client
        .enroll_browser_device(did, pp)
        .await
        .map_err(|e| jserr(e.into()))?;
    SessionStorage::set(SS_KEY, key.seed_hex()).ok();
    Ok(())
}
