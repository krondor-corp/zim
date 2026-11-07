//! Vaults: the "Open a vault" screen. Lists every vault in the account
//! (the last-opened one marked) and lets the user pick one or create a
//! new one. With zero vaults it doubles as the "create your first
//! vault" empty state the workspace resolver redirects to.

use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;
use yew_router::prelude::*;

use crate::api::{fetch_vaults, VaultItem};
use crate::components::dialog::PromptDialog;
use crate::pages::vault::short_id;
use crate::pages::workspace::{create_vault, last_vault};
use crate::routes::Route;

#[derive(Properties, PartialEq)]
pub struct Props {
    /// The signed-in user's id — new vaults are shared with all their devices.
    pub user_id: String,
}

#[function_component(Vaults)]
pub fn vaults(props: &Props) -> Html {
    let error = use_state(String::new);
    // None = still loading.
    let list = use_state(|| None::<Vec<VaultItem>>);
    let navigator = use_navigator().expect("router context");

    {
        let list = list.clone();
        let error = error.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                match fetch_vaults().await {
                    Ok(v) => list.set(Some(v)),
                    Err(e) => {
                        error.set(e);
                        list.set(Some(Vec::new()));
                    }
                }
            });
            || ()
        });
    }

    let show_dialog = use_state(|| false);
    let open_dialog = {
        let show_dialog = show_dialog.clone();
        Callback::from(move |_: MouseEvent| show_dialog.set(true))
    };
    let dialog_html = if *show_dialog {
        let show_dialog = show_dialog.clone();
        let user_id = props.user_id.clone();
        let error = error.clone();
        html! {
            <PromptDialog title="New vault" label="Name"
                on_cancel={let s = show_dialog.clone(); Callback::from(move |_: ()| s.set(false))}
                on_submit={Callback::from(move |name: String| {
                    show_dialog.set(false);
                    let error = error.clone();
                    create_vault(name, user_id.clone(), navigator.clone(),
                        Callback::from(move |e: String| error.set(e)));
                })} />
        }
    } else {
        Html::default()
    };

    let error_html = if error.is_empty() {
        Html::default()
    } else {
        html! { <div class="error">{ (*error).clone() }</div> }
    };

    let body = match &*list {
        None => html! {
            <div class="doc-empty workspace-empty"><span>{ "Loading\u{2026}" }</span></div>
        },
        Some(v) if v.is_empty() => html! {
            <div class="doc-empty workspace-empty">
                <p>{ "No vaults yet. A vault is an encrypted, versioned folder that syncs to all your devices." }</p>
                <button type="button" class="tree-pane__new workspace-empty__cta" onclick={open_dialog}>
                    { "+ Create your first vault" }
                </button>
            </div>
        },
        Some(v) => {
            let last = last_vault();
            html! {
                <>
                    <h1>{ "Open a vault" }</h1>
                    <div class="settings-card vault-list">
                        { for v.iter().map(|item| {
                            let id = item.vault_id;
                            let id_str = id.to_string();
                            let name = if item.name.is_empty() { short_id(&id_str) } else { item.name.clone() };
                            html! {
                                <Link<Route> to={Route::Vault { id }} classes="vault-row">
                                    <span class="vault-row__name">{ name }</span>
                                    if last == Some(id) {
                                        <span class="device-row__badge">{ "Last opened" }</span>
                                    }
                                    <code class="vault-row__id">{ short_id(&id_str) }</code>
                                </Link<Route>>
                            }
                        }) }
                    </div>
                    <button type="button" class="tree-pane__new vault-list__new" onclick={open_dialog}>
                        { "+ New vault" }
                    </button>
                </>
            }
        }
    };

    html! {
        <div class="vaults-page">
            { error_html }
            { body }
            { dialog_html }
        </div>
    }
}
