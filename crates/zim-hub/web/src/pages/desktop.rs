//! Desktop entry: what runs instead of the hub's sign-in + web-key gate
//! when the SPA is inside the Tauri shell. The app is "signed in" as the
//! local device — its daemon identity — so there's no account to load.
//!
//! Boot asks the shell to init `$ZIM_HOME` (first run) and connect to
//! the daemon (starting it if needed). On a fresh identity it then
//! offers to pair with a hub; skipping is fine — vaults work locally.

use serde::Deserialize;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_router::prelude::*;

use crate::api::Me;
use crate::backend::desktop::invoke;
use crate::layouts::OnboardingShell;
use crate::routes::{switch, Route};

#[derive(Clone, PartialEq, Deserialize)]
struct Boot {
    peer_id: String,
    first_run: bool,
    hub_linked: bool,
}

#[derive(Clone, PartialEq, Deserialize)]
struct Pair {
    hub_url: String,
    verification_url: String,
    code: String,
}

#[derive(Clone, PartialEq)]
enum Stage {
    Booting,
    Failed(String),
    /// First run: offer hub login.
    Offer,
    Pairing(Pair),
    Ready,
}

/// The identity the pages see: the local device, no hub account.
fn local_me(boot: &Boot) -> Me {
    Me {
        user_id: "local".into(),
        host: "localhost".into(),
        email: String::new(),
        did: format!("did:key:{}", boot.peer_id),
        has_web_key: true,
    }
}

#[function_component(DesktopApp)]
pub fn desktop_app() -> Html {
    let stage = use_state(|| Stage::Booting);
    let boot = use_state(|| None::<Boot>);
    let attempt = use_state(|| 0u32);
    let hub_url = use_state(String::new);
    let error = use_state(String::new);

    {
        let stage = stage.clone();
        let boot = boot.clone();
        let hub_url = hub_url.clone();
        use_effect_with(*attempt, move |_| {
            stage.set(Stage::Booting);
            spawn_local(async move {
                match invoke::<Boot>("boot", serde_json::json!({})).await {
                    Ok(b) => {
                        if let Ok(url) =
                            invoke::<String>("default_hub", serde_json::json!({})).await
                        {
                            hub_url.set(url);
                        }
                        stage.set(if b.first_run && !b.hub_linked {
                            Stage::Offer
                        } else {
                            Stage::Ready
                        });
                        boot.set(Some(b));
                    }
                    Err(e) => stage.set(Stage::Failed(e)),
                }
            });
            || ()
        });
    }

    let retry = {
        let attempt = attempt.clone();
        Callback::from(move |_: MouseEvent| attempt.set(*attempt + 1))
    };
    let skip = {
        let stage = stage.clone();
        Callback::from(move |_: MouseEvent| stage.set(Stage::Ready))
    };
    let on_url = {
        let hub_url = hub_url.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            hub_url.set(input.value());
        })
    };
    let connect = {
        let stage = stage.clone();
        let hub_url = hub_url.clone();
        let error = error.clone();
        Callback::from(move |_: MouseEvent| {
            let stage = stage.clone();
            let error = error.clone();
            let url = (*hub_url).clone();
            error.set(String::new());
            spawn_local(async move {
                let args = serde_json::json!({ "hubUrl": url });
                match invoke::<Pair>("hub_login_start", args).await {
                    Ok(pair) => {
                        stage.set(Stage::Pairing(pair));
                        // Blocks until the user approves on the hub.
                        match invoke::<serde_json::Value>(
                            "hub_login_complete",
                            serde_json::json!({}),
                        )
                        .await
                        {
                            Ok(_) => stage.set(Stage::Ready),
                            Err(e) => {
                                error.set(e);
                                stage.set(Stage::Offer);
                            }
                        }
                    }
                    Err(e) => error.set(e),
                }
            });
        })
    };

    let err_html = if error.is_empty() {
        Html::default()
    } else {
        html! { <div class="error">{ (*error).clone() }</div> }
    };

    match (&*stage, &*boot) {
        (Stage::Ready, Some(b)) => {
            let me = local_me(b);
            html! {
                <BrowserRouter>
                    <Switch<Route> render={move |route| switch(route, &me)} />
                </BrowserRouter>
            }
        }
        (Stage::Failed(e), _) => html! {
            <OnboardingShell>
                <div class="error">{ e.clone() }</div>
                <button type="button" class="btn" onclick={retry}>{ "Try again" }</button>
            </OnboardingShell>
        },
        (Stage::Offer, _) => html! {
            <OnboardingShell>
                <h2>{ "Welcome to zim" }</h2>
                <p class="muted">
                    { "Your vaults live on this device. Pair with a hub to mirror them to your other devices \u{2014} or skip and do it later with " }
                    <code>{ "zim hub login" }</code>{ "." }
                </p>
                <label class="muted">{ "Hub" }</label>
                <input type="text" value={(*hub_url).clone()} oninput={on_url} />
                { err_html }
                <div style="display:flex;gap:0.5rem;margin-top:1rem;">
                    <button type="button" class="btn" onclick={connect}>{ "Connect to hub" }</button>
                    <button type="button" class="btn" onclick={skip}>{ "Skip for now" }</button>
                </div>
            </OnboardingShell>
        },
        (Stage::Pairing(p), _) => html! {
            <OnboardingShell>
                <h2>{ "Approve this device" }</h2>
                <p class="muted">{ format!("Your browser should have opened {}. If not, open:", p.hub_url) }</p>
                <p><code style="word-break:break-all;">{ p.verification_url.clone() }</code></p>
                <p class="muted">{ "and confirm the code" }</p>
                <p><code style="font-size:1.6rem;letter-spacing:0.15em;">{ p.code.clone() }</code></p>
                <p class="muted">{ "Waiting for approval\u{2026}" }</p>
            </OnboardingShell>
        },
        _ => html! {
            <OnboardingShell><p class="muted">{ "Starting the zim daemon\u{2026}" }</p></OnboardingShell>
        },
    }
}
