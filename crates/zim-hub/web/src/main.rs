//! zim-hub Yew frontend.
//!
//! A Rust/WASM SPA that consumes `zim-hub-wasm` as an SDK (`HubClient` /
//! `WasmFs` / crypto called directly as Rust). The hub owns sign-in (its
//! server-rendered `/auth/*` pages), the session cookie, and the JSON API —
//! it only serves this app to signed-in, approved users. The app owns key
//! custody (the web-key gate) and the vault UI.
//!
//! Structure: `api/` (typed backend client) · `util/` (helpers) ·
//! `components/` (presentational lib) · `layouts/` (page shells) ·
//! `pages/` (route targets) · `routes.rs` (URL → page). `main.rs` only
//! loads the user, runs the web-key gate, and mounts the router.

mod api;
mod components;
mod layouts;
mod pages;
mod routes;
mod util;

use api::Me;
use layouts::OnboardingShell;
use pages::gate::KeyGate;
use routes::{switch, Route};

use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;

#[function_component(App)]
fn app() -> Html {
    // `None` = loading; `Some(None)` = unauthenticated; `Some(Some)` = user.
    let me = use_state(|| None::<Option<Me>>);
    let ready = use_state(|| false);

    {
        let me = me.clone();
        use_effect_with((), move |_| {
            let me = me.clone();
            spawn_local(async move {
                me.set(Some(api::fetch_me().await.unwrap_or(None)));
            });
            || ()
        });
    }

    let user = match &*me {
        None => {
            return html! {
                <OnboardingShell><p class="muted">{ "Loading\u{2026}" }</p></OnboardingShell>
            };
        }
        Some(None) => {
            // The hub only serves this app with a session, so this is one
            // that expired mid-use: sign in again, then come back here.
            if let Some(w) = web_sys::window() {
                let loc = w.location();
                let here = format!(
                    "{}{}",
                    loc.pathname().unwrap_or_default(),
                    loc.search().unwrap_or_default()
                );
                let next = String::from(js_sys::encode_uri_component(&here));
                let _ = loc.set_href(&format!("/auth/login?next={next}"));
            }
            return html! {};
        }
        Some(Some(user)) => user.clone(),
    };

    // Blocking web-key gate before the app.
    if !*ready {
        let on_ready = {
            let ready = ready.clone();
            Callback::from(move |_| ready.set(true))
        };
        return html! {
            <OnboardingShell email={user.email.clone()}>
                <KeyGate me={user} {on_ready} />
            </OnboardingShell>
        };
    }

    html! {
        <BrowserRouter>
            <Switch<Route> render={move |route| switch(route, &user)} />
        </BrowserRouter>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    yew::Renderer::<App>::new().render();
}
