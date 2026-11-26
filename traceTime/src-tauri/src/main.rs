#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod store;
mod tracker;

use tracker::Tracker;

fn main() {
    let data_dir = directories::ProjectDirs::from("", "", "traceTime")
        .expect("cannot resolve data dir")
        .data_dir()
        .to_path_buf();
    std::fs::create_dir_all(&data_dir).expect("cannot create data dir");
    let conn = store::open(&data_dir.join("traceTime.db")).expect("cannot open db");

    tauri::Builder::default()
        .manage(Tracker::new(conn))
        .invoke_handler(tauri::generate_handler![
            commands::create_task,
            commands::pause,
            commands::resume,
            commands::finish_task,
            commands::abandon_task,
            commands::current_task,
            commands::list_finished,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
