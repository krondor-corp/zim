//! The pack document pane: centered `editor-container` with the big
//! borderless title and ONE always-editable surface — no view/edit
//! modes. Text lives in a page-colored textarea; the parent holds the
//! draft and shows Save in the app header when changes are detected.
//! Non-text files render inline (image/pdf/video/audio).

use yew::prelude::*;

/// What the editor pane holds.
#[derive(Clone, PartialEq)]
pub struct OpenFile {
    pub path: String,
    pub mime: String,
    /// Object URL over the decrypted bytes (media preview + download).
    pub url: String,
    /// Decoded text for textual files.
    pub text: Option<String>,
}

#[derive(Properties, PartialEq)]
pub struct EditorProps {
    pub file: OpenFile,
    /// Every edit streams the draft up; the parent decides when it's
    /// dirty and renders the Save affordance in the header.
    pub on_change: Callback<String>,
}

#[function_component(EditorPane)]
pub fn editor_pane(props: &EditorProps) -> Html {
    let file = &props.file;
    let name = file.path.rsplit('/').next().unwrap_or_default().to_string();
    let title = name
        .rsplit_once('.')
        .map(|(stem, _)| stem.to_string())
        .unwrap_or_else(|| name.clone());
    let editable = file.text.is_some();

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
                <h1 class="editor-title" title={file.path.clone()}>{ title }</h1>
            </div>
            {
                if editable {
                    html! {
                        <textarea class="editor-textarea--doc" key={file.path.clone()}
                            defaultvalue={file.text.clone().unwrap_or_default()}
                            {oninput} spellcheck="false" placeholder="Write\u{2026}" />
                    }
                } else {
                    media_preview(file)
                }
            }
        </div>
    }
}

fn media_preview(file: &OpenFile) -> Html {
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
