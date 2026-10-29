//! The `.md` editor: Milkdown (Crepe) WYSIWYG behind the same props as the
//! textarea (`EditorProps`: the file + `on_change`). The bundle is a
//! lazily-imported static asset, fetched on the first `.md` open so the
//! SPA's initial load doesn't carry it. Like the textarea it is uncontrolled:
//! mounted once per `(path, rev)` with the loaded text, then left to the
//! user; the parent owns saving.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use yew::prelude::*;

use zim_hub_web::markdown::is_edit;

use crate::pages::vault_editor::EditorProps;

#[wasm_bindgen(inline_js = r#"
let loaded;
function ensureCss() {
  if (document.getElementById("milkdown-css")) return;
  const link = document.createElement("link");
  link.id = "milkdown-css";
  link.rel = "stylesheet";
  link.href = "/static/milkdown/milkdown.css";
  document.head.appendChild(link);
}
// Fetch the Crepe bundle + its stylesheet on first use only.
export async function mount(el, markdown, onChange) {
  ensureCss();
  loaded ??= import("/static/milkdown/milkdown.js");
  return (await loaded).mount(el, markdown, onChange);
}
"#)]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn mount(
        el: &web_sys::Element,
        markdown: &str,
        on_change: &Closure<dyn FnMut(String)>,
    ) -> Result<JsValue, JsValue>;
}

/// A mounted Crepe instance.
#[wasm_bindgen]
extern "C" {
    type Handle;
    #[wasm_bindgen(method, js_name = getMarkdown)]
    fn get_markdown(this: &Handle) -> String;
    #[wasm_bindgen(method)]
    fn destroy(this: &Handle) -> js_sys::Promise;
}

/// What a mount/unmount race needs to remember.
#[derive(Default)]
struct Session {
    handle: Option<Handle>,
    baseline: Option<String>,
    /// The effect's cleanup ran (the note changed or the pane closed).
    closed: bool,
}

#[function_component(MarkdownEditor)]
pub fn markdown_editor(props: &EditorProps) -> Html {
    let file = &props.file;
    let root = use_node_ref();
    {
        let root = root.clone();
        let on_change = props.on_change.clone();
        let text = file.text.clone().unwrap_or_default();
        use_effect_with((file.path.clone(), file.rev), move |_| {
            let session = Rc::new(RefCell::new(Session::default()));
            let cb = {
                let session = session.clone();
                Closure::<dyn FnMut(String)>::new(move |md: String| {
                    if is_edit(session.borrow().baseline.as_deref(), &md) {
                        on_change.emit(md);
                    }
                })
            };
            if let Some(el) = root.cast::<web_sys::Element>() {
                let session = session.clone();
                spawn_local(async move {
                    let res = mount(&el, &text, &cb).await;
                    match res {
                        Ok(h) => {
                            let h: Handle = h.unchecked_into();
                            if session.borrow().closed {
                                let _ = h.destroy();
                                return;
                            }
                            let mut s = session.borrow_mut();
                            s.baseline = Some(h.get_markdown());
                            s.handle = Some(h);
                            // `cb` must outlive the editor: leak it to the
                            // page's lifetime (one small closure per open).
                            cb.forget();
                        }
                        Err(e) => web_sys::console::error_1(&e),
                    }
                });
            }
            move || {
                let mut s = session.borrow_mut();
                s.closed = true;
                if let Some(h) = s.handle.take() {
                    let _ = h.destroy();
                }
            }
        });
    }

    html! {
        <div class="milkdown-wrap" key={format!("{}#{}", file.path, file.rev)} ref={root} />
    }
}
