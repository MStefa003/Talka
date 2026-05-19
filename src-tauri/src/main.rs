// Tauri requires this entry point on Windows to prevent a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    talka_lib::run();
}
