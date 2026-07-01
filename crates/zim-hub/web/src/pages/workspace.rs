//! Workspace: the vault list + "New vault". The list comes from
//! `/api/v0/vaults` (cookie-authed); creation goes through `WasmFs::init`
//! (browser key as owner), then the list reloads.

use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;
use zim_wasm::WasmFs;

use crate::api::{fetch_vaults, VaultItem};
use crate::routes::Route;
use crate::util::{jserr, origin};

#[derive(Properties, PartialEq)]
pub struct Props {
    /// The signed-in user's id — new vaults are shared with all their devices.
    pub user_id: String,
}

#[function_component(Workspace)]
pub fn workspace(props: &Props) -> Html {
    let vaults = use_state(|| None::<Vec<VaultItem>>);
    let error = use_state(String::new);
    let creating = use_state(|| false);

    let reload = {
        let vaults = vaults.clone();
        let error = error.clone();
        Callback::from(move |_: ()| {
            let vaults = vaults.clone();
            let error = error.clone();
            spawn_local(async move {
                match fetch_vaults().await {
                    Ok(v) => vaults.set(Some(v)),
                    Err(e) => error.set(e),
                }
            });
        })
    };

    {
        let reload = reload.clone();
        use_effect_with((), move |_| {
            reload.emit(());
            || ()
        });
    }

    let new_vault = {
        let creating = creating.clone();
        let reload = reload.clone();
        let error = error.clone();
        let user_id = props.user_id.clone();
        Callback::from(move |_: MouseEvent| {
            let name = web_sys::window()
                .and_then(|w| w.prompt_with_message("New vault name:").ok().flatten())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let Some(name) = name else {
                return;
            };
            let creating = creating.clone();
            let reload = reload.clone();
            let error = error.clone();
            let user_id = user_id.clone();
            creating.set(true);
            error.set(String::new());
            spawn_local(async move {
                match WasmFs::init(name, origin(), user_id).await {
                    Ok(_) => {
                        creating.set(false);
                        reload.emit(());
                    }
                    Err(e) => {
                        creating.set(false);
                        error.set(jserr(e.into()));
                    }
                }
            });
        })
    };

    html! {
        <>
            <span class="page-eyebrow">{ "workspace" }</span>
            <h1>{ "Your vaults" }</h1>
            <div style="display:flex;align-items:center;gap:0.75rem;margin:0 0 1rem;">
                <button class="btn btn-primary" onclick={new_vault} disabled={*creating}>
                    { if *creating { "Creating\u{2026}" } else { "New vault" } }
                </button>
            </div>
            if !(*error).is_empty() {
                <div class="error">{ (*error).clone() }</div>
            }
            {
                match &*vaults {
                    None => html! { <p class="muted">{ "Loading\u{2026}" }</p> },
                    Some(list) if list.is_empty() => html! {
                        <div class="section-empty">{ "No vaults yet. Click New vault to create one." }</div>
                    },
                    Some(list) => html! {
                        <div class="card-grid">
                            { for list.iter().map(|v| html! {
                                <Link<Route> classes="card" to={Route::Vault { id: v.vault_id.clone() }}>
                                    <h3>{ v.name.clone() }</h3>
                                    <span class="card-meta">{ v.vault_id.clone() }</span>
                                </Link<Route>>
                            }) }
                        </div>
                    },
                }
            }
        </>
    }
}
