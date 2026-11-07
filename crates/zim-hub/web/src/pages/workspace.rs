//! Workspace: no screen — a resolver. Sign-in lands here and gets
//! redirected straight into a vault: the last one opened (localStorage)
//! when it still exists, else the first in the account. With zero
//! vaults it redirects to the vault picker (`pages::vaults`), which
//! renders the "create your first vault" empty state. Choosing a vault
//! explicitly ("Open a vault") goes to that picker, never through here.

use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;
use zim_core::vault::VaultId;
use zim_wasm::WasmFs;

use crate::api::fetch_vaults;
use crate::routes::Route;
use crate::util::{jserr, origin};

const LAST_VAULT_KEY: &str = "zim.last_vault";

/// Remember the vault the user is working in (read back by the
/// workspace resolver on next sign-in).
pub fn remember_vault(id: &VaultId) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(LAST_VAULT_KEY, &id.to_string());
    }
}

/// The vault last remembered by [`remember_vault`], if any.
pub fn last_vault() -> Option<VaultId> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(LAST_VAULT_KEY).ok().flatten())
        .and_then(|v| v.parse().ok())
}

/// Create a vault with `name` and navigate into it. Shared by the
/// vault picker and the switcher's "+ New vault" dialog.
pub fn create_vault(name: String, user_id: String, navigator: Navigator, error: Callback<String>) {
    spawn_local(async move {
        match WasmFs::init(name, origin(), user_id).await {
            Ok(fs) => {
                let id = fs.id();
                remember_vault(&id);
                navigator.push(&Route::Vault { id });
            }
            Err(e) => error.emit(jserr(e.into())),
        }
    });
}

#[function_component(Workspace)]
pub fn workspace() -> Html {
    let error = use_state(String::new);
    let navigator = use_navigator().expect("router context");

    {
        let error = error.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                match fetch_vaults().await {
                    Ok(list) => {
                        // Last-opened when it still exists, else first;
                        // none at all → the picker's empty state.
                        let ids: Vec<VaultId> = list.iter().map(|v| v.vault_id).collect();
                        let target = last_vault()
                            .filter(|id| ids.contains(id))
                            .or_else(|| ids.first().copied());
                        match target {
                            Some(id) => navigator.replace(&Route::Vault { id }),
                            None => navigator.replace(&Route::Vaults),
                        }
                    }
                    Err(e) => error.set(e),
                }
            });
            || ()
        });
    }

    html! {
        <div class="doc-empty workspace-empty">
            if (*error).is_empty() {
                <span>{ "Loading\u{2026}" }</span>
            } else {
                <div class="error">{ (*error).clone() }</div>
            }
        </div>
    }
}
