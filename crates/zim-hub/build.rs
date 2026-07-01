//! Build script: ensure the SPA dist dir exists at compile time.
//!
//! The Yew SPA is built by Trunk (`bin/build-web`, run from `bin/hub` /
//! `make hub` / `./bin/dev hub`) into `web/dist/`, which `http::spa` embeds
//! via `rust_embed`. We don't shell out to trunk here: trunk is a nested
//! `cargo` invocation under the parent build and deadlocks on lock
//! contention. So all this script does is guarantee `web/dist/` exists
//! (rust_embed requires the folder at compile time) and nudge you to build
//! it if it's empty.

use std::path::PathBuf;

fn main() {
    // `rust_embed` (`http::spa`) needs `web/dist/` to exist at compile time.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let dist = PathBuf::from(&manifest_dir).join("web/dist");
    std::fs::create_dir_all(&dist).ok();

    let empty = std::fs::read_dir(&dist)
        .map(|mut d| d.next().is_none())
        .unwrap_or(true);
    if empty {
        println!(
            "cargo:warning=web/dist is empty — run ./bin/build-web (or use ./bin/hub / make hub)"
        );
    }
}
