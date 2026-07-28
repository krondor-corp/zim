//! The pack document pane: `editor-body` scroll region → centered
//! `editor-container` (44rem) with a big borderless title, an
//! Edit/Read tab cluster + explicit Save, and the document itself —
//! a page-colored textarea in Edit, rendered markdown (`.prose`,
//! pulldown-cmark in-wasm) in Read. Media renders inline.

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
    Read,
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
    /// Dirty-state changes, surfaced in the app header's save-status.
    pub on_dirty: Callback<bool>,
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
    let md = is_markdown(&file.path);

    // Buffer state: seeded from the file, owned by the editor until Save.
    let buffer = use_state(|| file.text.clone().unwrap_or_default());
    let dirty = use_state(|| false);
    let tab = use_state(|| if md { Tab::Read } else { Tab::Edit });

    // Re-seed when a different file is opened.
    {
        let buffer = buffer.clone();
        let dirty = dirty.clone();
        let tab = tab.clone();
        let on_dirty = props.on_dirty.clone();
        let text = file.text.clone().unwrap_or_default();
        let seed_tab = if md { Tab::Read } else { Tab::Edit };
        use_effect_with(file.path.clone(), move |_| {
            buffer.set(text);
            dirty.set(false);
            on_dirty.emit(false);
            tab.set(seed_tab);
            || ()
        });
    }

    let oninput = {
        let buffer = buffer.clone();
        let dirty = dirty.clone();
        let on_dirty = props.on_dirty.clone();
        Callback::from(move |e: InputEvent| {
            let el: web_sys::HtmlTextAreaElement = e.target_unchecked_into();
            buffer.set(el.value());
            if !*dirty {
                dirty.set(true);
                on_dirty.emit(true);
            }
        })
    };

    let save = {
        let on_save = props.on_save.clone();
        let on_dirty = props.on_dirty.clone();
        let path = file.path.clone();
        let buffer = buffer.clone();
        let dirty = dirty.clone();
        Callback::from(move |_: MouseEvent| {
            on_save.emit((path.clone(), (*buffer).clone()));
            dirty.set(false);
            on_dirty.emit(false);
        })
    };

    let set_tab = |t: Tab, state: &UseStateHandle<Tab>| {
        let state = state.clone();
        Callback::from(move |_: MouseEvent| state.set(t))
    };

    html! {
        <div class="editor-container">
            <div class="editor-title-row">
                <h1 class="editor-title" title={file.path.clone()}>
                    { title }
                    if *dirty {
                        <span class="editor-dirty-dot">{ "\u{25CF}" }</span>
                    }
                </h1>
                if editable {
                    <div class="editor-tabs">
                        if md {
                            <button type="button"
                                class={classes!("editor-tab", "editor-tab--btn", (*tab == Tab::Read).then_some("editor-tab--active"))}
                                title="Read" onclick={set_tab(Tab::Read, &tab)}>{ "\u{1F441}" }</button>
                        }
                        <button type="button"
                            class={classes!("editor-tab", "editor-tab--btn", (*tab == Tab::Edit).then_some("editor-tab--active"))}
                            title="Edit" onclick={set_tab(Tab::Edit, &tab)}>{ "\u{270E}" }</button>
                        <button type="button" class="editor-tab editor-tab--btn editor-tab--save"
                            disabled={!*dirty} onclick={save}>{ "Save" }</button>
                    </div>
                }
            </div>
            <div class="editor-metadata" id="editor-metadata"></div>
            {
                if editable {
                    match *tab {
                        Tab::Edit => html! {
                            <textarea class="editor-textarea--doc" value={(*buffer).clone()} {oninput}
                                spellcheck="false" placeholder="Write\u{2026}" />
                        },
                        Tab::Read => {
                            if md {
                                html! { <div class="prose">{ render_markdown(&buffer) }</div> }
                            } else {
                                html! { <pre class="prose" style="white-space:pre-wrap;">{ (*buffer).clone() }</pre> }
                            }
                        }
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
