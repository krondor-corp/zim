//! URL routes → pages. Each page is wrapped in its layout here; pages
//! themselves render only their content.

use yew::prelude::*;
use yew_router::prelude::*;
use zim_core::vault::VaultId;

use crate::api::Me;
use crate::backend::is_desktop;
use crate::layouts::PackChrome;
use crate::pages;

#[derive(Clone, Routable, PartialEq)]
pub enum Route {
    #[at("/")]
    Workspace,
    #[at("/v/:id")]
    Vault { id: VaultId },
    #[at("/settings")]
    Settings,
    #[at("/admin")]
    Admin,
    #[not_found]
    #[at("/404")]
    NotFound,
}

/// Render the page for `route`. `me` is threaded in from the authenticated
/// `App` so pages don't each re-fetch it.
pub fn switch(route: Route, me: &Me) -> Html {
    // The desktop device is its own account: there are no settings or
    // admin pages to show, so those URLs fall back to the vaults.
    let route = match route {
        Route::Settings | Route::Admin if is_desktop() => Route::Workspace,
        other => other,
    };
    match route {
        Route::Workspace => html! {
            <PackChrome crumb="vaults">
                <pages::workspace::Workspace user_id={me.user_id.clone()} />
            </PackChrome>
        },
        // The vault workspace is pack's editor-layout: its own fixed
        // header + tree + document chrome, no AppShell sidebar.
        Route::Vault { id } => html! {
            <pages::vault::VaultTree vault_id={id} user_id={me.user_id.clone()} />
        },
        Route::Settings => html! {
            <PackChrome crumb="settings">
                <pages::settings::Settings me={me.clone()} />
            </PackChrome>
        },
        Route::Admin => html! {
            <PackChrome crumb="admin">
                <pages::admin::Admin />
            </PackChrome>
        },
        Route::NotFound => html! { <pages::not_found::NotFound /> },
    }
}
