fn main() {
    // No commands: the page can't call this plugin, only the app's Rust can.
    tauri_plugin::Builder::new(&[]).ios_path("ios").build();
}
