// Keep the Windows release build from opening a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    zim_desktop::run()
}
