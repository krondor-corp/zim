//! Settings: a standalone shell (no app icon-rail) for account + device
//! management — its own grouped nav with a "back to app" link. Sections are
//! tabs rendered into a single centred column.

use yew::prelude::*;
use yew_router::prelude::*;

use crate::api::Me;
use crate::components::field;
use crate::pages::devices::Devices;
use crate::routes::Route;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    General,
    Devices,
}

#[derive(Properties, PartialEq)]
pub struct Props {
    pub me: Me,
}

#[function_component(Settings)]
pub fn settings(props: &Props) -> Html {
    let tab = use_state(|| Tab::General);

    let nav_to = {
        let tab = tab.clone();
        move |target: Tab| {
            let tab = tab.clone();
            Callback::from(move |e: MouseEvent| {
                e.prevent_default();
                tab.set(target);
            })
        }
    };

    let link = |target: Tab, icon: &str, label: &str| -> Html {
        let active = *tab == target;
        html! {
            <a href="#"
                class={classes!("settings-link", active.then_some("settings-link--active"))}
                onclick={nav_to(target)}>
                <span class="settings-link__icon">{ icon }</span>
                <span>{ label }</span>
            </a>
        }
    };

    let content = match *tab {
        Tab::General => html! { <General me={props.me.clone()} /> },
        Tab::Devices => html! { <Devices /> },
    };

    html! {
        <div class="settings-shell">
            <aside class="settings-nav">
                <Link<Route> to={Route::Workspace} classes="settings-nav__back">
                    <span class="settings-link__icon">{ "\u{2190}" }</span>
                    <span>{ "Back to app" }</span>
                </Link<Route>>
                <div class="settings-group">{ "Account" }</div>
                { link(Tab::General, "\u{2699}\u{FE0F}", "General") }
                { link(Tab::Devices, "\u{1F5A5}\u{FE0F}", "Devices") }
            </aside>
            <main class="settings-main">
                <div class="settings-main__inner">{ content }</div>
            </main>
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct GeneralProps {
    me: Me,
}

#[function_component(General)]
fn general(props: &GeneralProps) -> Html {
    let me = &props.me;
    let did = format!("did:web:{}:u:{}", me.host, me.user_id);

    html! {
        <>
            <span class="page-eyebrow">{ "settings" }</span>
            <h1>{ "General" }</h1>
            <p class="page-lead">{ "Your account identity on this hub." }</p>

            <section class="settings-card">
                <div class="settings-card__head">
                    <h2>{ "Account" }</h2>
                    <p>{ "Read-only identity details for your account on this hub." }</p>
                </div>
                <div class="settings-card__body">
                    { field("Email", &me.email, false) }
                    { field("User ID", &me.user_id, true) }
                    { field("Host", &me.host, false) }
                    { field("DID", &did, true) }
                </div>
            </section>

            <section class="settings-card">
                <div class="settings-card__head">
                    <h2>{ "Session" }</h2>
                    <p>{ "Sign out of this browser. Your web key stays safe in escrow \u{2014} unlock it again anytime." }</p>
                </div>
                <div class="settings-card__foot">
                    <form method="post" action="/auth/logout" style="margin:0;">
                        <button type="submit" class="btn btn-danger">{ "Sign out" }</button>
                    </form>
                </div>
            </section>
        </>
    }
}
