//! Platform-free pieces of the hub SPA, split out of the `wasm32`-only
//! binary so `cargo test --lib` runs them natively.

pub mod save_queue;
