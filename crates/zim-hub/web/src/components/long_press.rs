//! `use_long_press`: touch events → a "held" callback, the touch stand-in
//! for right-click (touch browsers, iOS above all, don't fire
//! `contextmenu`). The hold/drift/cancel rules are the lib's [`Press`].

use web_sys::EventTarget;
use yew::prelude::*;

use crate::util::sleep;
use zim_hub_web::long_press::{Press, HOLD_MS};

/// A finger was held still: where (viewport px), and what it pressed.
pub struct Held {
    pub x: i32,
    pub y: i32,
    pub target: Option<EventTarget>,
}

/// The touch handlers to spread on the pressable element.
pub struct LongPress {
    pub start: Callback<TouchEvent>,
    pub moved: Callback<TouchEvent>,
    pub end: Callback<TouchEvent>,
    pub cancel: Callback<TouchEvent>,
}

/// Fires `on_held` when one finger stays down and still for [`HOLD_MS`];
/// the tap that ends such a hold is swallowed.
#[hook]
pub fn use_long_press(on_held: Callback<Held>) -> LongPress {
    let press = use_mut_ref(Press::default);
    let start = {
        let press = press.clone();
        Callback::from(move |e: TouchEvent| {
            let touches = e.touches();
            let touch = match touches.get(0) {
                Some(t) if touches.length() == 1 => t,
                _ => return press.borrow_mut().cancel(),
            };
            let target = e.target();
            let generation = press.borrow_mut().start(touch.client_x(), touch.client_y());
            let press = press.clone();
            let on_held = on_held.clone();
            yew::platform::spawn_local(async move {
                sleep(HOLD_MS).await;
                let held = press.borrow_mut().elapsed(generation);
                if let Some((x, y)) = held {
                    on_held.emit(Held { x, y, target });
                }
            });
        })
    };
    let moved = {
        let press = press.clone();
        Callback::from(move |e: TouchEvent| {
            if let Some(t) = e.touches().get(0) {
                press.borrow_mut().moved(t.client_x(), t.client_y());
            }
        })
    };
    let end = {
        let press = press.clone();
        Callback::from(move |e: TouchEvent| {
            // The hold already acted: don't also tap what's under it.
            if press.borrow_mut().end() {
                e.prevent_default();
            }
        })
    };
    let cancel = Callback::from(move |_: TouchEvent| press.borrow_mut().cancel());
    LongPress {
        start,
        moved,
        end,
        cancel,
    }
}
