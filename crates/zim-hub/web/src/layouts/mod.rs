//! Layout library — the page shells. A layout wraps a page's content with
//! chrome (nav, topbar); pages render only their content.

use yew::prelude::*;
use yew_router::prelude::*;

use crate::routes::Route;

/// The authenticated app chrome: pinned sidebar + topbar. Active nav is
/// derived from the current route. Wraps `workspace` and `vault` pages.
#[derive(Properties, PartialEq)]
pub struct AppShellProps {
    pub email: String,
    pub children: Html,
}

#[function_component(AppShell)]
pub fn app_shell(props: &AppShellProps) -> Html {
    let route = use_route::<Route>().unwrap_or(Route::Workspace);
    let in_workspace = matches!(route, Route::Workspace | Route::Vault { .. });
    let in_settings = matches!(route, Route::Settings);

    html! {
        <div class="app-shell">
            <aside class="sidebar sidebar--pinned">
                <div class="sidebar__top">
                    <Link<Route> to={Route::Workspace} classes="sidebar__brand">
                        <span class="sidebar__brand-text">{ "zim" }</span>
                    </Link<Route>>
                </div>
                <div class="sidebar__body">
                    <nav class="sidebar__nav">
                        <Link<Route> to={Route::Workspace}
                            classes={classes!("sidebar__link", in_workspace.then_some("sidebar__link--active"))}>
                            <span class="sidebar__label">{ "Workspace" }</span>
                        </Link<Route>>
                        <Link<Route> to={Route::Settings}
                            classes={classes!("sidebar__link", in_settings.then_some("sidebar__link--active"))}>
                            <span class="sidebar__label">{ "Settings" }</span>
                        </Link<Route>>
                    </nav>
                </div>
                <div class="sidebar__footer">
                    <form method="post" action="/auth/logout" style="margin:0;">
                        <button type="submit" class="sidebar__link"
                            style="background:none;border:none;width:100%;text-align:left;cursor:pointer;font:inherit;color:inherit;">
                            <span class="sidebar__label">{ "Sign out" }</span>
                        </button>
                    </form>
                </div>
            </aside>
            <header class="app-topbar">
                <span class="who">{ props.email.clone() }</span>
            </header>
            <main class="app-main"><div class="app-main__inner">{ props.children.clone() }</div></main>
        </div>
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
                    <Link<Route> to={Route::Workspace} classes="app-menu__item">{ "Workspace" }</Link<Route>>
                    <Link<Route> to={Route::Settings} classes="app-menu__item">{ "Settings" }</Link<Route>>
                    <a href="/auth/logout" class="app-menu__item">{ "Sign out" }</a>
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
