//! The pack document pane: centered `editor-container` with the big
//! borderless title and ONE always-editable surface — no view/edit
//! modes. Text lives in a page-colored textarea that only reports its
//! changes; the parent owns saving (autosave). Any editor that emits
//! `on_change` with the full text can stand in for the textarea.
//! `.md` files swap the textarea for Milkdown (`markdown_editor`).
//! Non-text files render inline (image/pdf/video/audio).

use yew::prelude::*;

use zim_hub_web::markdown::is_markdown;

use crate::pages::markdown_editor::MarkdownEditor;

/// What the editor pane holds.
#[derive(Clone, PartialEq)]
pub struct OpenFile {
    pub path: String,
    pub mime: String,
    /// Object URL over the decrypted bytes (media preview + download).
    pub url: String,
    /// Decoded text for textual files.
    pub text: Option<String>,
    /// Fingerprint of the decrypted bytes: tells a remote edit to this
    /// file apart from a head change that left it untouched.
    pub digest: u64,
    /// Unique per load. The textarea is uncontrolled (its text is written
    /// once per load, then left to the user), so it's keyed on this:
    /// every (re)load remounts it with the freshly read text. An autosave
    /// keeps it, so saving never disturbs the cursor.
    pub rev: u32,
}

#[derive(Properties, PartialEq)]
pub struct EditorProps {
    pub file: OpenFile,
    /// Every edit streams the full text up; the parent queues and
    /// autosaves it.
    pub on_change: Callback<String>,
}

/// The pane around an editor: [`EditorProps`] plus the editable title.
#[derive(Properties, PartialEq)]
pub struct EditorPaneProps {
    pub file: OpenFile,
    pub on_change: Callback<String>,
    /// The title was edited: the file's new name (extension kept).
    pub on_rename: Callback<String>,
}

#[function_component(EditorPane)]
pub fn editor_pane(props: &EditorPaneProps) -> Html {
    let file = &props.file;
    let name = file.path.rsplit('/').next().unwrap_or_default().to_string();
    // The title is the name without its extension; renaming keeps it.
    let (title, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), format!(".{ext}")),
        _ => (name.clone(), String::new()),
    };
    let editable = file.text.is_some();

    // The title is an uncontrolled input too (written once per path), so
    // a re-render mid-typing never resets it.
    let title_ref = use_node_ref();
    {
        let title_ref = title_ref.clone();
        let title = title.clone();
        use_effect_with(file.path.clone(), move |_| {
            if let Some(el) = title_ref.cast::<web_sys::HtmlInputElement>() {
                el.set_value(&title);
            }
            || ()
        });
    }
    // Enter or leaving the field renames; Escape puts the name back.
    let commit_title = {
        let on_rename = props.on_rename.clone();
        let title = title.clone();
        Callback::from(move |e: FocusEvent| {
            let el: web_sys::HtmlInputElement = e.target_unchecked_into();
            let typed = el.value();
            if typed.trim().is_empty() || typed.trim() == title {
                el.set_value(&title);
            } else {
                on_rename.emit(format!("{}{ext}", typed.trim()));
            }
        })
    };
    let title_keys = {
        let title = title.clone();
        Callback::from(move |e: KeyboardEvent| {
            let el: web_sys::HtmlInputElement = e.target_unchecked_into();
            match e.key().as_str() {
                "Enter" => {
                    e.prevent_default();
                    let _ = el.blur();
                }
                "Escape" => {
                    el.set_value(&title);
                    let _ = el.blur();
                }
                _ => {}
            }
        })
    };

    // Write the loaded text into the textarea once per load. Not via
    // `value=`: Yew re-asserts that against the DOM on every render,
    // clobbering whatever the user typed since.
    let textarea = use_node_ref();
    {
        let textarea = textarea.clone();
        let text = file.text.clone().unwrap_or_default();
        use_effect_with((file.path.clone(), file.rev), move |_| {
            if let Some(el) = textarea.cast::<web_sys::HtmlTextAreaElement>() {
                el.set_value(&text);
            }
            || ()
        });
    }

    let oninput = {
        let on_change = props.on_change.clone();
        Callback::from(move |e: InputEvent| {
            let el: web_sys::HtmlTextAreaElement = e.target_unchecked_into();
            on_change.emit(el.value());
        })
    };

    html! {
        <div class="editor-container">
            <div class="editor-title-row">
                <input class="editor-title editor-title--input" key={file.path.clone()}
                    ref={title_ref} title={file.path.clone()} aria-label="Note name"
                    spellcheck="false" autocomplete="off" enterkeyhint="done"
                    onblur={commit_title} onkeydown={title_keys} />
            </div>
            {
                if editable && is_markdown(&file.path) {
                    html! { <MarkdownEditor file={file.clone()} on_change={props.on_change.clone()} /> }
                } else if editable {
                    html! {
                        <textarea class="editor-textarea--doc" key={format!("{}#{}", file.path, file.rev)}
                            ref={textarea}
                            {oninput} spellcheck="false" placeholder="Write\u{2026}" />
                    }
                } else {
                    media_preview(file)
                }
            }
        </div>
    }
}

pub(crate) fn media_preview(file: &OpenFile) -> Html {
    let url = file.url.clone();
    let name = file.path.rsplit('/').next().unwrap_or_default().to_string();
    if file.mime.starts_with("image/") {
        html! { <img class="editor-media" src={url} alt={name} /> }
    } else if file.mime == "application/pdf" {
        html! { <iframe class="editor-frame" src={url} title={name}></iframe> }
    } else if file.mime.starts_with("video/") {
        html! { <video class="editor-media" src={url} controls={true} /> }
    } else if file.mime.starts_with("audio/") {
        html! { <audio style="width:100%;" src={url} controls={true} /> }
    } else {
        html! {
            <p class="doc-empty">
                { format!("No inline preview for {} \u{2014} ", file.mime) }
                <a href={file.url.clone()} download={name} style="margin-left:0.3rem;">{ "download" }</a>
            </p>
        }
    }
}
