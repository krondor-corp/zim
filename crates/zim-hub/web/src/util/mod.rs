//! SPA-level utilities — no UI, no backend. Plain helpers shared across
//! pages and components.

use wasm_bindgen::JsValue;

/// sessionStorage key for the unlocked web-key seed.
pub const SS_KEY: &str = "zim:webkey";

/// Same-origin base for the hub API (cookies + JWT audience).
pub fn origin() -> String {
    web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_default()
}

/// Best-effort message out of a `JsError`/`JsValue`.
pub fn jserr(e: JsValue) -> String {
    js_sys::Error::from(e)
        .message()
        .as_string()
        .unwrap_or_else(|| "error".to_string())
}

/// Random hex suffix for a did fragment label (disambiguator, not secret).
pub fn rand_suffix() -> String {
    format!("{:08x}", (js_sys::Math::random() * 4_294_967_296.0) as u32)
}
