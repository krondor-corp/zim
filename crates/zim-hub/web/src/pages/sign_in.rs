//! Sign-in landing page for anonymous visitors: one entry per login
//! provider the hub has enabled (`GET /api/v0/auth/providers`). Each
//! entry is a plain link or form — the provider's own route mints the
//! session cookie and redirects back to `/`.

use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use crate::api::{self, SignInOption};

#[function_component(SignIn)]
pub fn sign_in() -> Html {
    // `None` = loading.
    let options = use_state(|| None::<Result<Vec<SignInOption>, String>>);

    {
        let options = options.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                options.set(Some(api::fetch_auth_providers().await));
            });
            || ()
        });
    }

    let body = match &*options {
        None => html! { <p class="dim">{ "Loading\u{2026}" }</p> },
        Some(Err(e)) => html! { <div class="error">{ format!("Couldn't load sign-in options: {e}") }</div> },
        Some(Ok(list)) if list.is_empty() => {
            html! { <div class="error">{ "This hub has no sign-in options configured." }</div> }
        }
        Some(Ok(list)) => list.iter().enumerate().map(|(i, o)| html! {
            <>
                if i > 0 { <p class="dim" style="text-align:center;margin:1rem 0;">{ "or" }</p> }
                { option(o) }
            </>
        }).collect::<Html>(),
    };

    html! {
        <div class="gate" style="max-width:440px;margin:0 auto;">
            <h3 style="margin-top:0;">{ "Sign in" }</h3>
            { body }
        </div>
    }
}

fn option(o: &SignInOption) -> Html {
    match o {
        SignInOption::Redirect { label, path, .. } => html! {
            <a class="btn btn-primary" href={path.clone()} style="width:100%;text-align:center;">
                { label.clone() }
            </a>
        },
        SignInOption::Email {
            label,
            action,
            default_email,
            warning,
            ..
        } => html! {
            <form method="post" action={action.clone()}>
                if let Some(w) = warning {
                    <div class="error" style="margin-bottom:0.75rem;">{ w.clone() }</div>
                }
                <div class="field">
                    <label>{ "Email" }</label>
                    <input type="email" name="email" required=true
                        value={default_email.clone().unwrap_or_default()} />
                </div>
                <button type="submit" class="btn btn-primary" style="width:100%;">{ label.clone() }</button>
            </form>
        },
    }
}
