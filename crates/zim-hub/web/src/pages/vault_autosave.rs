//! Autosave driver: runs the writes a [`SaveQueue`] hands out, and wires
//! the page-lifecycle moments that must flush (tab hidden, tab close,
//! Cmd/Ctrl+S). The editor only reports text; the page owns saving.

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use yew::prelude::*;
use zim_hub_web::save_queue::SaveQueue;

use crate::backend::VaultBackend;
use crate::util::sleep;

/// How long an edit may sit unsaved: the first edit after a write arms
/// one timer, and everything typed until it fires lands in one write.
/// Every write is a version, so this bounds history density.
pub(crate) const AUTOSAVE_MS: i32 = 30_000;

/// How often [`settled`] re-checks a draining queue.
const SETTLE_POLL_MS: i32 = 50;

pub(crate) type Queue = Rc<RefCell<SaveQueue>>;

/// What the write loop reports back to the page.
pub(crate) enum SaveEvent {
    Saving,
    Saved { path: String, text: String },
    Failed(String),
}

/// The header's save indicator.
#[derive(Clone, PartialEq)]
pub(crate) enum SaveState {
    Idle,
    Saving,
    /// Shown briefly, then faded out by CSS.
    Saved,
    Failed(String),
}

/// Run flushed writes one at a time until the queue has none to hand
/// out. Safe to call whenever: if a write is already running, its loop
/// picks up the flush and this returns at once.
pub(crate) async fn drain(backend: &VaultBackend, queue: &Queue, on: &Callback<SaveEvent>) {
    loop {
        let next = queue.borrow_mut().start();
        let Some((path, text)) = next else {
            return;
        };
        on.emit(SaveEvent::Saving);
        let result = backend.write(path.clone(), text.clone().into_bytes()).await;
        queue.borrow_mut().finish(result.is_ok());
        match result {
            Ok(()) => on.emit(SaveEvent::Saved { path, text }),
            Err(e) => {
                on.emit(SaveEvent::Failed(e));
                return;
            }
        }
    }
}

/// Wait until the queue has nothing in flight or wanted — before an
/// operation (move, delete, vault switch) that must not race a write.
pub(crate) async fn settled(queue: &Queue) {
    while !queue.borrow().is_settled() {
        sleep(SETTLE_POLL_MS).await;
    }
}

/// Flush on the moments a tab can go away under unsaved text — hidden
/// (tab switch, app switch, mobile background), `beforeunload`, and
/// `pagehide` — plus Cmd/Ctrl+S as an explicit "save now". A closing
/// page may not live to finish the write, so `beforeunload` also asks the
/// browser to confirm leaving while a draft is still unwritten.
#[hook]
pub(crate) fn use_lifecycle_flush(flush: Callback<()>, queue: Queue) {
    use_effect_with((), move |_| {
        let window = web_sys::window().expect("window");
        let document = window.document().expect("document");

        let on_hidden = {
            let flush = flush.clone();
            let document = document.clone();
            Closure::<dyn Fn(web_sys::Event)>::new(move |_| {
                if document.hidden() {
                    flush.emit(());
                }
            })
        };
        let on_beforeunload = {
            let flush = flush.clone();
            Closure::<dyn Fn(web_sys::Event)>::new(move |e: web_sys::Event| {
                flush.emit(());
                if !queue.borrow().is_settled() {
                    e.prevent_default();
                }
            })
        };
        let on_pagehide = {
            let flush = flush.clone();
            Closure::<dyn Fn(web_sys::Event)>::new(move |_| flush.emit(()))
        };
        let on_key =
            Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
                if (e.meta_key() || e.ctrl_key()) && e.key().eq_ignore_ascii_case("s") {
                    e.prevent_default();
                    flush.emit(());
                }
            });

        let _ = document.add_event_listener_with_callback(
            "visibilitychange",
            on_hidden.as_ref().unchecked_ref(),
        );
        let _ = window.add_event_listener_with_callback(
            "beforeunload",
            on_beforeunload.as_ref().unchecked_ref(),
        );
        let _ = window
            .add_event_listener_with_callback("pagehide", on_pagehide.as_ref().unchecked_ref());
        let _ = window.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());

        move || {
            let _ = document.remove_event_listener_with_callback(
                "visibilitychange",
                on_hidden.as_ref().unchecked_ref(),
            );
            let _ = window.remove_event_listener_with_callback(
                "beforeunload",
                on_beforeunload.as_ref().unchecked_ref(),
            );
            let _ = window.remove_event_listener_with_callback(
                "pagehide",
                on_pagehide.as_ref().unchecked_ref(),
            );
            let _ = window
                .remove_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref());
        }
    });
}
