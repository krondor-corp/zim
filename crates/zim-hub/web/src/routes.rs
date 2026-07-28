//! URL routes → pages. Each page is wrapped in its layout here; pages
//! themselves render only their content.

use yew::prelude::*;
use yew_router::prelude::*;

use crate::api::Me;
use crate::layouts::PackChrome;
use crate::pages;

#[derive(Clone, Routable, PartialEq)]
pub enum Route {
    #[at("/")]
    Workspace,
    #[at("/v/:id")]
    Vault { id: String },
    #[at("/settings")]
    Settings,
    #[at("/admin")]
    Admin,
    #[at("/device")]
    DevicePair,
    #[not_found]
    #[at("/404")]
    NotFound,
}

/// Render the page for `route`. `me` is threaded in from the authenticated
/// `App` so pages don't each re-fetch it.
pub fn switch(route: Route, me: &Me) -> Html {
    match route {
        Route::Workspace => html! {
            <PackChrome crumb="vaults">
                <pages::workspace::Workspace user_id={me.user_id.clone()} />
            </PackChrome>
        },
        // The vault workspace is pack's editor-layout: its own fixed
        // header + tree + document chrome, no AppShell sidebar.
        Route::Vault { id } => html! {
            <pages::vault::VaultTree vault_id={id} />
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
        // Device pairing has its own bare layout (reached pre-app from a
        // `zim hub login` link).
        Route::DevicePair => html! { <pages::device_pair::DevicePair /> },
        Route::NotFound => html! { <pages::not_found::NotFound /> },
    }
}
