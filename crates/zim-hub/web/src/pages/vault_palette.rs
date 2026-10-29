//! The vault page's Cmd/Ctrl+K command palette: a modal with one text
//! box over a single fuzzy-filtered list of the vault's files (the whole
//! tree) and the page's actions. The palette only chooses; the page
//! performs the choice through the callbacks it passes in.

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{HtmlInputElement, KeyboardEvent};
use yew::prelude::*;
use zim_hub_web::fuzzy;

use zim_core::vault::VaultId;
use zim_wasm::EntryKind;

use crate::api::VaultItem;
use crate::backend::VaultBackend;
use crate::pages::vault::short_id;

/// Most rows rendered at once; the query narrows the rest.
const MAX_ROWS: usize = 50;

/// What the page can do on the palette's behalf.
#[derive(Clone, Copy, PartialEq)]
pub enum PaletteAction {
    NewNote,
    NewFolder,
    DeleteOpenFile,
    SaveNow,
    History,
    Details,
    SwitchVault(VaultId),
}

/// One selectable row.
#[derive(Clone, PartialEq)]
enum Item {
    Action(PaletteAction, String),
    File(String),
}

impl Item {
    fn label(&self) -> &str {
        match self {
            Item::Action(_, label) => label,
            Item::File(path) => path.trim_start_matches('/'),
        }
    }
}

/// Every file in the vault, depth first. A directory that can't be
/// listed is skipped; the palette still serves what it could read.
async fn list_files(backend: &VaultBackend) -> Vec<String> {
    let mut files = Vec::new();
    let mut dirs = vec!["/".to_string()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = backend.ls(dir.clone()).await else {
            continue;
        };
        for e in entries {
            let path = if dir == "/" {
                format!("/{}", e.name)
            } else {
                format!("{dir}/{}", e.name)
            };
            match e.kind {
                EntryKind::Dir => dirs.push(path),
                EntryKind::File => files.push(path),
            }
        }
    }
    files.sort();
    files
}

/// Open the palette on Cmd/Ctrl+K from anywhere on the page. Crepe binds
/// no `Mod-k`, so the editor never swallows the key before it bubbles here.
#[hook]
pub fn use_palette_shortcut(open: Callback<()>) {
    use_effect_with((), move |_| {
        let window = web_sys::window().expect("window");
        let on_key = Closure::<dyn Fn(KeyboardEvent)>::new(move |e: KeyboardEvent| {
            if (e.meta_key() || e.ctrl_key()) && e.key().eq_ignore_ascii_case("k") {
                e.prevent_default();
                open.emit(());
            }
        });
        let _ = window.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
        move || {
            let _ = window
                .remove_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
        }
    });
}

#[derive(Properties, PartialEq)]
pub struct PaletteProps {
    pub backend: VaultBackend,
    pub vault_id: VaultId,
    pub vaults: Vec<VaultItem>,
    /// The open file's path; the delete action needs one.
    pub open_path: Option<String>,
    pub on_action: Callback<PaletteAction>,
    pub on_open: Callback<String>,
    pub on_close: Callback<()>,
}

#[function_component(Palette)]
pub fn palette(props: &PaletteProps) -> Html {
    let query = use_state(String::new);
    let selected = use_state(|| 0usize);
    let files = use_state(Vec::<String>::new);
    let input_ref = use_node_ref();
    let list_ref = use_node_ref();

    {
        let input_ref = input_ref.clone();
        let files = files.clone();
        let backend = props.backend.clone();
        use_effect_with((), move |_| {
            if let Some(input) = input_ref.cast::<HtmlInputElement>() {
                let _ = input.focus();
            }
            yew::platform::spawn_local(async move { files.set(list_files(&backend).await) });
            || ()
        });
    }

    let mut all = vec![
        Item::Action(PaletteAction::NewNote, "New note".into()),
        Item::Action(PaletteAction::NewFolder, "New folder".into()),
    ];
    if let Some(path) = &props.open_path {
        let name = path.rsplit('/').next().unwrap_or_default();
        all.push(Item::Action(
            PaletteAction::DeleteOpenFile,
            format!("Delete {name}"),
        ));
    }
    all.push(Item::Action(PaletteAction::SaveNow, "Save now".into()));
    all.push(Item::Action(
        PaletteAction::History,
        "Open version history".into(),
    ));
    all.push(Item::Action(
        PaletteAction::Details,
        "Open vault details".into(),
    ));
    for v in &props.vaults {
        let id = v.vault_id;
        if id != props.vault_id {
            let name = if v.name.is_empty() {
                short_id(&id.to_string())
            } else {
                v.name.clone()
            };
            all.push(Item::Action(
                PaletteAction::SwitchVault(id),
                format!("Switch to {name}"),
            ));
        }
    }
    all.extend(files.iter().cloned().map(Item::File));

    let labels: Vec<&str> = all.iter().map(Item::label).collect();
    let items: Vec<Item> = fuzzy::rank(&query, &labels)
        .into_iter()
        .take(MAX_ROWS)
        .map(|i| all[i].clone())
        .collect();
    let selected_ix = (*selected).min(items.len().saturating_sub(1));

    // Keep the highlighted row on screen as the arrows move it.
    {
        let list_ref = list_ref.clone();
        use_effect_with(selected_ix, move |_| {
            let on = list_ref
                .cast::<web_sys::Element>()
                .and_then(|list| list.query_selector(".palette__item--on").ok().flatten());
            if let Some(el) = on {
                let opts = web_sys::ScrollIntoViewOptions::new();
                opts.set_block(web_sys::ScrollLogicalPosition::Nearest);
                el.scroll_into_view_with_scroll_into_view_options(&opts);
            }
            || ()
        });
    }

    let choose = {
        let on_action = props.on_action.clone();
        let on_open = props.on_open.clone();
        let on_close = props.on_close.clone();
        Callback::from(move |item: Item| {
            on_close.emit(());
            match item {
                Item::Action(a, _) => on_action.emit(a),
                Item::File(path) => on_open.emit(path),
            }
        })
    };

    let oninput = {
        let query = query.clone();
        let selected = selected.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            query.set(input.value());
            selected.set(0);
        })
    };

    let onkeydown = {
        let items = items.clone();
        let selected = selected.clone();
        let choose = choose.clone();
        let on_close = props.on_close.clone();
        Callback::from(move |e: KeyboardEvent| match e.key().as_str() {
            "ArrowDown" => {
                e.prevent_default();
                selected.set((selected_ix + 1).min(items.len().saturating_sub(1)));
            }
            "ArrowUp" => {
                e.prevent_default();
                selected.set(selected_ix.saturating_sub(1));
            }
            "Enter" => {
                e.prevent_default();
                if let Some(item) = items.get(selected_ix) {
                    choose.emit(item.clone());
                }
            }
            "Escape" => on_close.emit(()),
            _ => {}
        })
    };

    let close_click = {
        let on_close = props.on_close.clone();
        Callback::from(move |_: MouseEvent| on_close.emit(()))
    };

    html! {
        <>
            <div class="dialog-backdrop" onclick={close_click}></div>
            <div class="dialog palette" role="dialog" aria-modal="true" aria-label="Command palette" {onkeydown}>
                <input ref={input_ref} class="palette__input" type="text" value={(*query).clone()}
                    placeholder="Search files and actions\u{2026}" spellcheck="false"
                    autocomplete="off" {oninput} />
                <div class="palette__list" ref={list_ref}>
                    if items.is_empty() {
                        <div class="palette__empty">{ "No matches" }</div>
                    }
                    { for items.iter().enumerate().map(|(i, item)| {
                        let choose = choose.clone();
                        let picked = item.clone();
                        let class = classes!("palette__item", (i == selected_ix).then_some("palette__item--on"));
                        let row = match item {
                            Item::Action(_, label) => html! {
                                <>
                                    <span class="palette__name">{ label.clone() }</span>
                                    <span class="palette__hint">{ "action" }</span>
                                </>
                            },
                            Item::File(path) => {
                                let (dir, name) = path.trim_start_matches('/').rsplit_once('/')
                                    .map_or(("", path.trim_start_matches('/')), |(d, n)| (d, n));
                                html! {
                                    <>
                                        <span class="palette__name">{ name.to_string() }</span>
                                        <span class="palette__hint">{ dir.to_string() }</span>
                                    </>
                                }
                            }
                        };
                        html! {
                            <button type="button" {class}
                                onclick={Callback::from(move |_: MouseEvent| choose.emit(picked.clone()))}>
                                { row }
                            </button>
                        }
                    }) }
                </div>
            </div>
        </>
    }
}
