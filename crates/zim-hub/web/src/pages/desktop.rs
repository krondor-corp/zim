//! Desktop entry: what runs instead of the hub's sign-in + web-key gate
//! when the SPA is inside the Tauri shell. The app is "signed in" as the
//! local device — its daemon identity — so there's no account to load.
//!
//! Boot asks the shell to init `$ZIM_HOME` (first run) and connect to
//! the daemon (starting it if needed). On a fresh identity it then
//! offers to pair with a hub; skipping is fine — vaults work locally.

use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_router::prelude::*;
use zim_api::desktop::{cmd, BootInfo, CommandError, LoginInfo, LoginStart, PairInfo};
use zim_api::Url;

use crate::api::Me;
use crate::backend::desktop::{call, call0};
use crate::layouts::OnboardingShell;
use crate::routes::{switch, Route};

#[derive(Clone, PartialEq)]
enum Stage {
    Booting,
    Failed(CommandError),
    /// First run: offer hub login.
    Offer,
    Pairing(Box<PairInfo>),
    Ready,
}

/// The identity the pages see: the local device, no hub account. The
/// fields are the hub's account shape; none apply here.
fn local_me() -> Me {
    Me {
        user_id: "local".into(),
        host: "localhost".into(),
        email: String::new(),
        did: String::new(),
        has_web_key: true,
    }
}

#[function_component(DesktopApp)]
pub fn desktop_app() -> Html {
    let stage = use_state(|| Stage::Booting);
    let attempt = use_state(|| 0u32);
    // What's typed in the hub field; parsed into a `Url` on connect.
    let hub_text = use_state(String::new);
    let error = use_state(|| None::<CommandError>);
    let bad_url = use_state(String::new);

    {
        let stage = stage.clone();
        let hub_text = hub_text.clone();
        use_effect_with(*attempt, move |_| {
            stage.set(Stage::Booting);
            spawn_local(async move {
                match call0::<BootInfo>(cmd::BOOT).await {
                    Ok(b) => {
                        if let Ok(url) = call0::<Url>(cmd::DEFAULT_HUB).await {
                            hub_text.set(url.to_string());
                        }
                        stage.set(if b.first_run && !b.hub_linked {
                            Stage::Offer
                        } else {
                            Stage::Ready
                        });
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
        let hub_text = hub_text.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            hub_text.set(input.value());
        })
    };
    let connect = {
        let stage = stage.clone();
        let hub_text = hub_text.clone();
        let error = error.clone();
        let bad_url = bad_url.clone();
        Callback::from(move |_: MouseEvent| {
            let hub_url = match Url::parse(hub_text.trim()) {
                Ok(u) => u,
                Err(e) => {
                    bad_url.set(format!("Not a valid hub URL: {e}"));
                    return;
                }
            };
            bad_url.set(String::new());
            error.set(None);
            let stage = stage.clone();
            let error = error.clone();
            spawn_local(async move {
                match call::<_, PairInfo>(cmd::HUB_LOGIN_START, &LoginStart { hub_url }).await {
                    Ok(pair) => {
                        stage.set(Stage::Pairing(Box::new(pair)));
                        // Blocks until the user approves on the hub.
                        match call0::<LoginInfo>(cmd::HUB_LOGIN_COMPLETE).await {
                            Ok(_) => stage.set(Stage::Ready),
                            Err(e) => {
                                error.set(Some(e));
                                stage.set(Stage::Offer);
                            }
                        }
                    }
                    Err(e) => error.set(Some(e)),
                }
            });
        })
    };

    let err_html = match (&*error, bad_url.is_empty()) {
        (Some(e), _) => html! { <div class="error">{ e.to_string() }</div> },
        (None, false) => html! { <div class="error">{ (*bad_url).clone() }</div> },
        (None, true) => Html::default(),
    };

    match &*stage {
        Stage::Ready => {
            let me = local_me();
            html! {
                <BrowserRouter>
                    <Switch<Route> render={move |route| switch(route, &me)} />
                </BrowserRouter>
            }
        }
        Stage::Failed(e) => html! {
            <OnboardingShell>
                <div class="error">{ e.to_string() }</div>
                <button type="button" class="btn" onclick={retry}>{ "Try again" }</button>
            </OnboardingShell>
        },
        Stage::Offer => html! {
            <OnboardingShell>
                <h2>{ "Welcome to zim" }</h2>
                <p class="muted">
                    { "Your vaults live on this device. Pair with a hub to mirror them to your other devices \u{2014} or skip and do it later with " }
                    <code>{ "zim hub login" }</code>{ "." }
                </p>
                <label class="muted">{ "Hub" }</label>
                <input type="text" value={(*hub_text).clone()} oninput={on_url} />
                { err_html }
                <div style="display:flex;gap:0.5rem;margin-top:1rem;">
                    <button type="button" class="btn" onclick={connect}>{ "Connect to hub" }</button>
                    <button type="button" class="btn" onclick={skip}>{ "Skip for now" }</button>
                </div>
            </OnboardingShell>
        },
        Stage::Pairing(p) => html! {
            <OnboardingShell>
                <h2>{ "Approve this device" }</h2>
                <p class="muted">{ format!("Your browser should have opened {}. If not, open:", p.hub_url) }</p>
                <p><code style="word-break:break-all;">{ p.verification_url.to_string() }</code></p>
                <p class="muted">{ "and confirm the code" }</p>
                <p><code style="font-size:1.6rem;letter-spacing:0.15em;">{ p.code.clone() }</code></p>
                <p class="muted">{ "Waiting for approval\u{2026}" }</p>
            </OnboardingShell>
        },
        Stage::Booting => html! {
            <OnboardingShell><p class="muted">{ "Starting the zim daemon\u{2026}" }</p></OnboardingShell>
        },
    }
}
