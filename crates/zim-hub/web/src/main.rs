//! zim-hub Yew frontend.
//!
//! A Rust/WASM SPA that consumes `zim-hub-wasm` as an SDK (`HubClient` /
//! `WasmFs` / crypto called directly as Rust). The hub owns OAuth + the
//! session cookie + the JSON API; this app drives everything client-side.
//!
//! Structure: `api/` (typed backend client) · `util/` (helpers) ·
//! `components/` (presentational lib) · `layouts/` (page shells) ·
//! `pages/` (route targets) · `routes.rs` (URL → page). `main.rs` only
//! does auth, the web-key gate, and mounts the router.

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
            if let Some(w) = web_sys::window() {
                let _ = w.location().set_href("/auth/google/login");
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
