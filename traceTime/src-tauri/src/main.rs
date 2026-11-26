// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

struct AppState;

fn main() {
    tauri::Builder::default()
        .manage(AppState)
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
