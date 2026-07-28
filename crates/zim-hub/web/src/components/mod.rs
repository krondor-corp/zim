//! Component library — small, reusable, presentational pieces. No fetching,
//! no routing; pages compose these.

pub mod dialog;

use yew::prelude::*;

/// A copy-to-clipboard button for a fixed string value. Fire-and-forget:
/// `navigator.clipboard.writeText` returns a Promise we don't await.
/// Note: the Clipboard API is only available on secure contexts
/// (https or localhost).
pub fn copy_button(value: &str) -> Html {
    let value = value.to_string();
    let onclick = Callback::from(move |_: MouseEvent| {
        if let Some(nav) = web_sys::window().map(|w| w.navigator()) {
            let _ = nav.clipboard().write_text(&value);
        }
    });
    html! {
        <button type="button" class="btn field-copy__btn" {onclick} title="Copy">
            { "\u{29C9}" }
        </button>
    }
}

/// A labelled read-only value, optionally with a copy-to-clipboard button
/// (used for the monospace identifiers — user id, DID).
pub fn field(label: &str, value: &str, copy: bool) -> Html {
    let input = html! {
        <input type="text" value={value.to_string()} readonly=true
            class={classes!(copy.then_some("mono"))} />
    };
    html! {
        <div class="field">
            <label>{ label }</label>
            if copy {
                <div class="field-copy">
                    { input }
                    { copy_button(value) }
                </div>
            } else {
                { input }
            }
        </div>
    }
}
