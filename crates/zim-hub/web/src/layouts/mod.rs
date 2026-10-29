//! Layout library — the page shells. A layout wraps a page's content with
//! chrome (nav, topbar); pages render only their content.

use yew::prelude::*;
use yew_router::prelude::*;

use crate::backend::is_desktop;
use crate::routes::Route;

/// "Sign out" as a header-menu item. Logout is POST-only (a GET link
/// would make it CSRF-trivial), so this is a one-button form. The click
/// doesn't bubble: the menu's close-on-click would re-render and could
/// drop the form before the browser submits it.
#[function_component(SignOutItem)]
pub fn sign_out_item() -> Html {
    let keep_open = Callback::from(|e: MouseEvent| e.stop_propagation());
    html! {
        <form method="post" action="/auth/logout" style="margin:0;">
            <button type="submit" class="app-menu__item" onclick={keep_open}>{ "Sign out" }</button>
        </form>
    }
}

/// Centered, chrome-light shell for pre-app states: loading and the
/// web-key gate.
#[derive(Properties, PartialEq)]
pub struct OnboardingShellProps {
    #[prop_or_default]
    pub email: String,
    pub children: Html,
}

#[function_component(OnboardingShell)]
pub fn onboarding_shell(props: &OnboardingShellProps) -> Html {
    html! {
        <div class="onb">
            <header class="onb__bar">
                <span class="onb__brand">{ "zim" }</span>
                if !props.email.is_empty() {
                    <span class="onb__who">
                        <span class="dim">{ props.email.clone() }</span>
                        <form method="post" action="/auth/logout" style="margin:0;">
                            <button type="submit" class="onb__signout">{ "Sign out" }</button>
                        </form>
                    </span>
                }
            </header>
            <main class="onb__main"><div class="onb__inner">{ props.children.clone() }</div></main>
        </div>
    }
}

/// A header dropdown (pack's app-menu) as Yew state instead of a
/// native `<details>`: in an SPA the DOM survives navigation, so a
/// details element would stay open across route changes. Clicking the
/// backdrop or ANY item closes it (items still navigate — close piggy-
/// backs on the bubbled click).
#[derive(Properties, PartialEq)]
pub struct HeaderMenuProps {
    /// The trigger's content (hamburger glyph, crumb text, …).
    pub trigger: Html,
    /// Extra classes on the wrapper (e.g. `vault-switch`).
    #[prop_or_default]
    pub class: Classes,
    /// Trigger classes; defaults to the icon-button look.
    #[prop_or("app-menu__trigger".into())]
    pub trigger_class: Classes,
    pub children: Html,
}

#[function_component(HeaderMenu)]
pub fn header_menu(props: &HeaderMenuProps) -> Html {
    let open = use_state(|| false);
    let toggle = {
        let open = open.clone();
        Callback::from(move |_: MouseEvent| open.set(!*open))
    };
    let close = {
        let open = open.clone();
        Callback::from(move |_: MouseEvent| open.set(false))
    };
    html! {
        <div class={classes!("app-menu", props.class.clone(), (*open).then_some("app-menu--open"))}>
            <button type="button" class={props.trigger_class.clone()} onclick={toggle}>
                { props.trigger.clone() }
            </button>
            if *open {
                <div style="position:fixed;inset:0;z-index:59;" onclick={close.clone()}></div>
                <nav class="app-menu__list" onclick={close}>
                    { props.children.clone() }
                </nav>
            }
        </div>
    }
}

/// Pack's slim chrome for non-vault pages (Settings, Workspace, …):
/// the same fixed app-header the vault workspace renders, wrapping a
/// centered content column. One design language everywhere.
#[derive(Properties, PartialEq)]
pub struct PackChromeProps {
    /// Breadcrumb label for the header's left slot.
    pub crumb: String,
    pub children: Html,
}

#[function_component(PackChrome)]
pub fn pack_chrome(props: &PackChromeProps) -> Html {
    html! {
        <>
            <header class="app-header">
                <HeaderMenu trigger={html! { "\u{2630}" }}>
                    <Link<Route> to={Route::Workspace} classes="app-menu__item">{ "Open a vault" }</Link<Route>>
                    // Settings (devices, hub account) and sign-out are hub-only.
                    if !is_desktop() {
                        <Link<Route> to={Route::Settings} classes="app-menu__item">{ "Settings" }</Link<Route>>
                        <SignOutItem />
                    }
                </HeaderMenu>
                <div class="app-header__slot app-header__slot--left">
                    <span class="app-header__crumb app-header__crumb--current">{ props.crumb.clone() }</span>
                </div>
                <div class="app-header__slot app-header__slot--right"></div>
            </header>
            <main class="pack-main">
                { props.children.clone() }
            </main>
        </>
    }
}
