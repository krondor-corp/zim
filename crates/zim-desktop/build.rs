fn main() {
    // The sidecar's file name carries the target triple; the dev fallback
    // in `sidecar.rs` needs it at runtime.
    println!(
        "cargo:rustc-env=ZIM_DESKTOP_TARGET={}",
        std::env::var("TARGET").expect("cargo sets TARGET for build scripts")
    );
    tauri_build::build()
}
