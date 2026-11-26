#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod reminder;
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(Tracker::new(conn))
        .invoke_handler(tauri::generate_handler![
            commands::create_task,
            commands::pause,
            commands::resume,
            commands::finish_task,
            commands::abandon_task,
            commands::current_task,
            commands::list_finished,
            commands::list_paused,
            commands::task_detail,
            commands::get_setting,
            commands::set_setting,
            commands::check_reminders,
            commands::set_reminder_muted,
            commands::send_reminder_notification,
            commands::open_history,
            commands::open_settings,
            commands::set_widget_height,
            commands::delete_task,
            commands::delete_tasks,
            commands::save_text_file_via_dialog,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
