//! The editor pane of the vault workspace: pack-style document editor
//! with Edit/Preview tabs, explicit save, and inline previews for
//! non-text files. Markdown renders via pulldown-cmark (in-wasm — the
//! content is the user's own decrypted data, same trust domain as the
//! editor itself).

use yew::prelude::*;

/// What the editor pane holds.
#[derive(Clone, PartialEq)]
pub struct OpenFile {
    pub path: String,
    pub mime: String,
    /// Object URL over the decrypted bytes (media preview + download).
    pub url: String,
    /// Decoded text for textual files (edit + markdown preview).
    pub text: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Edit,
    Preview,
}

fn render_markdown(src: &str) -> Html {
    let parser = pulldown_cmark::Parser::new_ext(src, pulldown_cmark::Options::all());
    let mut out = String::new();
    pulldown_cmark::html::push_html(&mut out, parser);
    Html::from_html_unchecked(AttrValue::from(out))
}

fn is_markdown(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.ends_with(".md") || p.ends_with(".markdown")
}

#[derive(Properties, PartialEq)]
pub struct EditorProps {
    pub file: OpenFile,
    /// Save the buffer back to the vault (explicit — no autosave).
    pub on_save: Callback<(String, String)>,
    pub on_close: Callback<()>,
    /// "saving…" / error text from the parent, shown by the actions.
    pub status: String,
}

#[function_component(EditorPane)]
pub fn editor_pane(props: &EditorProps) -> Html {
    let file = &props.file;
    let name = file.path.rsplit('/').next().unwrap_or_default().to_string();
    let editable = file.text.is_some();
    let md = is_markdown(&file.path);

    // Buffer state: seeded from the file, owned by the editor until Save.
    let buffer = use_state(|| file.text.clone().unwrap_or_default());
    let dirty = use_state(|| false);
    let tab = use_state(|| if md && editable { Tab::Preview } else { Tab::Edit });

    // Re-seed when a different file is opened.
    {
        let buffer = buffer.clone();
        let dirty = dirty.clone();
        let tab = tab.clone();
        let text = file.text.clone().unwrap_or_default();
        let seed_tab = if md { Tab::Preview } else { Tab::Edit };
        use_effect_with(file.path.clone(), move |_| {
            buffer.set(text);
            dirty.set(false);
            tab.set(seed_tab);
            || ()
        });
    }

    let oninput = {
        let buffer = buffer.clone();
        let dirty = dirty.clone();
        Callback::from(move |e: InputEvent| {
            let el: web_sys::HtmlTextAreaElement = e.target_unchecked_into();
            buffer.set(el.value());
            dirty.set(true);
        })
    };

    let save = {
        let on_save = props.on_save.clone();
        let path = file.path.clone();
        let buffer = buffer.clone();
        let dirty = dirty.clone();
        Callback::from(move |_: MouseEvent| {
            on_save.emit((path.clone(), (*buffer).clone()));
            dirty.set(false);
        })
    };

    let close = {
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| on_close.emit(()))
    };

    let set_tab = |t: Tab, state: &UseStateHandle<Tab>| {
        let state = state.clone();
        Callback::from(move |_: MouseEvent| state.set(t))
    };

    html! {
        <div class="editor-container">
            <div class="editor-title-row">
                <span class="editor-title" title={file.path.clone()}>{ name.clone() }</span>
                if *dirty {
                    <span class="editor-dirty" title="Unsaved changes">{ "\u{25CF}" }</span>
                }
                <span class="editor-spacer"></span>
                if editable {
                    if md {
                        <button type="button"
                            class={classes!("editor-tab", (*tab == Tab::Edit).then_some("editor-tab--active"))}
                            onclick={set_tab(Tab::Edit, &tab)}>{ "Edit" }</button>
                        <button type="button"
                            class={classes!("editor-tab", (*tab == Tab::Preview).then_some("editor-tab--active"))}
                            onclick={set_tab(Tab::Preview, &tab)}>{ "Preview" }</button>
                    }
                    <button type="button" class="btn btn--primary editor-save"
                        disabled={!*dirty} onclick={save}>{ "Save" }</button>
                }
                <a class="btn" href={file.url.clone()} download={name}>{ "Download" }</a>
                <button type="button" class="btn" onclick={close}>{ "Close" }</button>
            </div>
            if !props.status.is_empty() {
                <div class="editor-status muted">{ props.status.clone() }</div>
            }
            <div class="editor-body">
                {
                    if editable {
                        match *tab {
                            Tab::Edit => html! {
                                <textarea class="editor-textarea" value={(*buffer).clone()} {oninput}
                                    spellcheck="false" />
                            },
                            Tab::Preview => {
                                if md {
                                    html! { <div class="prose">{ render_markdown(&buffer) }</div> }
                                } else {
                                    html! { <pre class="editor-plain">{ (*buffer).clone() }</pre> }
                                }
                            }
                        }
                    } else {
                        media_preview(file)
                    }
                }
            </div>
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
        html! { <p class="muted editor-nopreview">{ format!("No inline preview for {} \u{2014} use Download.", file.mime) }</p> }
    }
}
