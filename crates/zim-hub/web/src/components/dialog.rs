//! First-class dialogs (pack's `.dialog` design) replacing the
//! browser's `prompt()`/`confirm()`: a fixed centered panel over a
//! blurred backdrop, Enter submits, Escape/backdrop cancels, the input
//! autofocuses. Rendered from page-level state — pages hold
//! `Option<SomeDialog>` and map submit/cancel back to their actions.

use wasm_bindgen::JsCast;
use web_sys::{HtmlInputElement, KeyboardEvent};
use yew::prelude::*;

/// A one-field prompt dialog ("New note", "New folder", "New vault").
#[derive(Properties, PartialEq)]
pub struct PromptDialogProps {
    pub title: String,
    pub label: String,
    /// Pre-filled value (e.g. a suggested name); fully selected on focus.
    #[prop_or_default]
    pub value: String,
    #[prop_or("Create".to_string())]
    pub action: String,
    pub on_submit: Callback<String>,
    pub on_cancel: Callback<()>,
}

#[function_component(PromptDialog)]
pub fn prompt_dialog(props: &PromptDialogProps) -> Html {
    let input_ref = use_node_ref();

    // Autofocus + select the seed value once mounted.
    {
        let input_ref = input_ref.clone();
        use_effect_with((), move |_| {
            if let Some(input) = input_ref.cast::<HtmlInputElement>() {
                let _ = input.focus();
                input.select();
            }
            || ()
        });
    }

    let submit = {
        let input_ref = input_ref.clone();
        let on_submit = props.on_submit.clone();
        Callback::from(move |_| {
            let Some(input) = input_ref.cast::<HtmlInputElement>() else {
                return;
            };
            let value = input.value().trim().to_string();
            if !value.is_empty() {
                on_submit.emit(value);
            }
        })
    };

    let onkeydown = {
        let submit = submit.clone();
        let on_cancel = props.on_cancel.clone();
        Callback::from(move |e: KeyboardEvent| match e.key().as_str() {
            "Enter" => {
                e.prevent_default();
                submit.emit(());
            }
            "Escape" => on_cancel.emit(()),
            _ => {}
        })
    };

    let cancel_click = {
        let on_cancel = props.on_cancel.clone();
        Callback::from(move |_: MouseEvent| on_cancel.emit(()))
    };
    let submit_click = {
        let submit = submit.clone();
        Callback::from(move |_: MouseEvent| submit.emit(()))
    };

    html! {
        <>
            <div class="dialog-backdrop" onclick={cancel_click.clone()}></div>
            <div class="dialog" role="dialog" aria-modal="true" {onkeydown}>
                <div class="dialog__form">
                    <h2 class="dialog__title">{ props.title.clone() }</h2>
                    <label class="dialog__label">{ props.label.clone() }</label>
                    <input ref={input_ref} class="dialog__input" type="text"
                        value={props.value.clone()} spellcheck="false" />
                    <div class="dialog__actions">
                        <button type="button" class="dialog__btn" onclick={cancel_click}>{ "Cancel" }</button>
                        <button type="button" class="dialog__btn dialog__btn--primary" onclick={submit_click}>
                            { props.action.clone() }
                        </button>
                    </div>
                </div>
            </div>
        </>
    }
}

/// A confirm dialog ("Delete x?") — destructive action styling.
#[derive(Properties, PartialEq)]
pub struct ConfirmDialogProps {
    pub title: String,
    pub body: String,
    #[prop_or("Delete".to_string())]
    pub action: String,
    pub on_confirm: Callback<()>,
    pub on_cancel: Callback<()>,
}

#[function_component(ConfirmDialog)]
pub fn confirm_dialog(props: &ConfirmDialogProps) -> Html {
    let confirm_ref = use_node_ref();
    {
        let confirm_ref = confirm_ref.clone();
        use_effect_with((), move |_| {
            if let Some(el) = confirm_ref.cast::<web_sys::HtmlElement>() {
                let _ = el.focus();
            }
            || ()
        });
    }
    let cancel = {
        let cb = props.on_cancel.clone();
        Callback::from(move |_: MouseEvent| cb.emit(()))
    };
    let confirm = {
        let cb = props.on_confirm.clone();
        Callback::from(move |_: MouseEvent| cb.emit(()))
    };
    let onkeydown = {
        let on_cancel = props.on_cancel.clone();
        Callback::from(move |e: KeyboardEvent| {
            if e.key() == "Escape" {
                on_cancel.emit(());
            }
        })
    };
    html! {
        <>
            <div class="dialog-backdrop" onclick={cancel.clone()}></div>
            <div class="dialog" role="alertdialog" aria-modal="true" {onkeydown}>
                <div class="dialog__form">
                    <h2 class="dialog__title">{ props.title.clone() }</h2>
                    <p class="dialog__body">{ props.body.clone() }</p>
                    <div class="dialog__actions">
                        <button type="button" class="dialog__btn" onclick={cancel}>{ "Cancel" }</button>
                        <button ref={confirm_ref} type="button"
                            class="dialog__btn dialog__btn--danger" onclick={confirm}>
                            { props.action.clone() }
                        </button>
                    </div>
                </div>
            </div>
        </>
    }
}

/// Convenience: does this keyboard event's target sit inside a dialog?
/// (Pages with global key handlers use it to stay out of the way.)
pub fn in_dialog(e: &KeyboardEvent) -> bool {
    e.target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        .map(|el| el.closest(".dialog").ok().flatten().is_some())
        .unwrap_or(false)
}
