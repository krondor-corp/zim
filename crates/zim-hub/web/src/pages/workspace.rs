//! Workspace: no screen — a resolver. Sign-in lands here and gets
//! redirected straight into a vault: the last one opened (localStorage)
//! when it still exists, else the first in the account. With zero
//! vaults it renders the one vestige of the old list page: a
//! "create your first vault" empty state. Creation itself goes through
//! `WasmFs::init` (browser key as owner) and is also reachable from
//! the vault switcher's "+ New vault".

use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;
use zim_wasm::WasmFs;

use crate::api::fetch_vaults;
use crate::routes::Route;
use crate::util::{jserr, origin};

const LAST_VAULT_KEY: &str = "zim.last_vault";

/// Remember the vault the user is working in (read back by the
/// workspace resolver on next sign-in).
pub fn remember_vault(id: &str) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(LAST_VAULT_KEY, id);
    }
}

fn last_vault() -> Option<String> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(LAST_VAULT_KEY).ok().flatten())
        .filter(|v| !v.is_empty())
}

/// Prompt for a name and create a vault; navigates into it on success.
/// Shared by the empty state here and the switcher's "+ New vault".
pub fn create_vault_flow(
    user_id: String,
    navigator: Navigator,
    error: Callback<String>,
) {
    let name = web_sys::window()
        .and_then(|w| w.prompt_with_message("New vault name:").ok().flatten())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let Some(name) = name else {
        return;
    };
    spawn_local(async move {
        match WasmFs::init(name, origin(), user_id).await {
            Ok(fs) => {
                let id = fs.vault_id();
                remember_vault(&id);
                navigator.push(&Route::Vault { id });
            }
            Err(e) => error.emit(jserr(e.into())),
        }
    });
}

#[derive(Properties, PartialEq)]
pub struct Props {
    /// The signed-in user's id — new vaults are shared with all their devices.
    pub user_id: String,
}

#[function_component(Workspace)]
pub fn workspace(props: &Props) -> Html {
    let error = use_state(String::new);
    // None = still resolving; Some(true) = no vaults (show empty state).
    let empty = use_state(|| None::<bool>);
    let navigator = use_navigator().expect("router context");

    {
        let empty = empty.clone();
        let error = error.clone();
        let navigator = navigator.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                match fetch_vaults().await {
                    Ok(list) if list.is_empty() => empty.set(Some(true)),
                    Ok(list) => {
                        // Last-opened when it still exists, else first.
                        let target = last_vault()
                            .filter(|id| list.iter().any(|v| &v.vault_id == id))
                            .unwrap_or_else(|| list[0].vault_id.clone());
                        navigator.replace(&Route::Vault { id: target });
                    }
                    Err(e) => {
                        error.set(e);
                        empty.set(Some(true));
                    }
                }
            });
            || ()
        });
    }

    let create = {
        let user_id = props.user_id.clone();
        let navigator = navigator.clone();
        let error = error.clone();
        Callback::from(move |_: MouseEvent| {
            let error = error.clone();
            create_vault_flow(
                user_id.clone(),
                navigator.clone(),
                Callback::from(move |e: String| error.set(e)),
            );
        })
    };

    html! {
        <div class="doc-empty" style="min-height:60vh;flex-direction:column;gap:1rem;">
            if !(*error).is_empty() {
                <div class="error">{ (*error).clone() }</div>
            }
            {
                match *empty {
                    None => html! { <span>{ "Loading\u{2026}" }</span> },
                    Some(_) => html! {
                        <>
                            <p>{ "No vaults yet. A vault is an encrypted, versioned folder that syncs to all your devices." }</p>
                            <button type="button" class="tree-pane__new" style="padding:0.5rem 1.2rem;" onclick={create}>
                                { "+ Create your first vault" }
                            </button>
                        </>
                    },
                }
            }
        </div>
    }
}
