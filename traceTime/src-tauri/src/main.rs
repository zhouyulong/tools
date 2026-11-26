#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod reminder;
mod store;
mod tracker;

use chrono::Utc;
use tauri::Manager;
use tracker::{BackgroundMode, Tracker};

fn persist_widget_positions(
    receiver: std::sync::mpsc::Receiver<(i32, i32)>,
    mut persist: impl FnMut(i32, i32),
) {
    for (x, y) in receiver {
        persist(x, y);
    }
}

fn main() {
    let data_dir = directories::ProjectDirs::from("", "", "traceTime")
        .expect("cannot resolve data dir")
        .data_dir()
        .to_path_buf();
    std::fs::create_dir_all(&data_dir).expect("cannot create data dir");
    let conn = store::open(&data_dir.join("traceTime.db")).expect("cannot open db");
    let tracker = Tracker::new(conn);
    tracker
        .recover_running_tasks(Utc::now())
        .expect("cannot recover running tasks");
    let widget_position = tracker
        .widget_position()
        .expect("cannot read widget position");
    let background_mode = tracker.background_mode().unwrap_or(BackgroundMode::Glass);
    let (position_sender, position_receiver) = std::sync::mpsc::channel();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(tracker)
        .setup(move |app| {
            let app_handle = app.handle().clone();
            std::thread::Builder::new()
                .name("widget-position-writer".into())
                .spawn(move || {
                    persist_widget_positions(position_receiver, |x, y| {
                        if let Err(error) = app_handle.state::<Tracker>().set_widget_position(x, y)
                        {
                            eprintln!("cannot save widget position: {error}");
                        }
                    });
                })?;

            let window = app
                .get_webview_window("main")
                .ok_or("main window not found")?;
            if let Some((x, y)) = widget_position {
                window.set_position(tauri::PhysicalPosition::new(x, y))?;
            }
            commands::apply_background_mode_to_window(&window, background_mode);
            Ok(())
        })
        .on_window_event(move |window, event| {
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::Moved(position) = event
                && let Err(error) = position_sender.send((position.x, position.y))
            {
                eprintln!("cannot queue widget position: {error}");
            }
        })
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
            commands::set_settings,
            commands::apply_background_mode,
            commands::check_reminders,
            commands::set_reminder_muted,
            commands::send_reminder_notification,
            commands::open_history,
            commands::open_settings,
            commands::set_widget_position,
            commands::set_widget_height,
            commands::delete_task,
            commands::delete_tasks,
            commands::save_text_file_via_dialog,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::persist_widget_positions;

    #[test]
    fn position_worker_applies_moves_in_send_order() {
        let (sender, receiver) = std::sync::mpsc::channel();
        sender.send((10, 20)).unwrap();
        sender.send((30, 40)).unwrap();
        sender.send((-50, 60)).unwrap();
        drop(sender);

        let mut written = Vec::new();
        persist_widget_positions(receiver, |x, y| written.push((x, y)));

        assert_eq!(written, vec![(10, 20), (30, 40), (-50, 60)]);
    }
}
