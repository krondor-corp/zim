//! Admin: user management. Lists users and exposes role controls
//! (authorize / unauthorize / promote / demote) against the admin API.
//! `RequireAdmin` gates the API, so a non-admin just sees a 403 error.

use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::api::{admin_action, fetch_admin_users, AdminUser, AdminUsers};

#[function_component(Admin)]
pub fn admin() -> Html {
    let data = use_state(|| None::<AdminUsers>);
    let error = use_state(String::new);

    let reload = {
        let data = data.clone();
        let error = error.clone();
        Callback::from(move |_: ()| {
            let data = data.clone();
            let error = error.clone();
            spawn_local(async move {
                match fetch_admin_users().await {
                    Ok(d) => data.set(Some(d)),
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

    let act = {
        let reload = reload.clone();
        let error = error.clone();
        Callback::from(move |(id, action): (String, String)| {
            let reload = reload.clone();
            let error = error.clone();
            spawn_local(async move {
                match admin_action(&id, &action).await {
                    Ok(()) => reload.emit(()),
                    Err(e) => error.set(e),
                }
            });
        })
    };

    html! {
        <>
            <span class="page-eyebrow">{ "admin" }</span>
            <h1>{ "Users" }</h1>
            <p class="page-lead">{ "Authorize accounts and manage admin roles." }</p>
            if !(*error).is_empty() {
                <div class="error">{ (*error).clone() }</div>
            }
            {
                match &*data {
                    None => html! { <p class="muted">{ "Loading\u{2026}" }</p> },
                    Some(d) => html! {
                        <div class="card" style="padding:0;overflow:hidden;">
                            { for d.users.iter().map(|u| user_row(u, &d.current_admin_id, &act)) }
                        </div>
                    },
                }
            }
        </>
    }
}

fn user_row(u: &AdminUser, current_admin_id: &str, act: &Callback<(String, String)>) -> Html {
    let is_self = u.id == current_admin_id;
    let btn = |label: &str, action: &str, danger: bool| -> Html {
        let id = u.id.clone();
        let action = action.to_string();
        let act = act.clone();
        let onclick = Callback::from(move |_: MouseEvent| act.emit((id.clone(), action.clone())));
        html! {
            <button type="button" class={classes!("btn", danger.then_some("btn-danger"))} {onclick}
                style="padding:0.1rem 0.5rem;font-size:0.75rem;">{ label }</button>
        }
    };
    html! {
        <div style="display:flex;gap:0.6rem;align-items:center;padding:0.6rem 1rem;border-bottom:1px solid hsl(var(--border));font-size:0.9rem;">
            <div style="display:flex;flex-direction:column;min-width:0;">
                <span>{ &u.email }</span>
                <span class="muted" style="font-size:0.75rem;">{ &u.name }</span>
            </div>
            <span style="flex:1;"></span>
            <span class="device-row__badge">{ &u.role }</span>
            if is_self {
                <span class="muted" style="font-size:0.72rem;">{ "you" }</span>
            } else {
                if u.is_authorized {
                    { btn("Unauthorize", "unauthorize", true) }
                } else {
                    { btn("Authorize", "authorize", false) }
                }
                if u.is_admin {
                    { btn("Demote", "demote", true) }
                } else {
                    { btn("Promote", "promote", false) }
                }
            }
        </div>
    }
}
