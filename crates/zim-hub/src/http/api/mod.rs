//! `/api` — ciphertext-serving HTTP for browser-side decryption.
//!
//! All endpoints here serve raw bytes or public log metadata. No
//! decryption happens on the hub. The browser is expected to fetch
//! ciphertext (blobs, manifests) and walk it client-side via zim-wasm.

pub mod v0;

use axum::Router;

use crate::state::AppState;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new().nest("/v0", v0::router(state))
}
