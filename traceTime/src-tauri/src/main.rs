#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod background;
mod commands;
mod position;
mod reminder;
mod store;
mod tracker;

use chrono::Utc;
use tauri::Manager;
use tracker::{BackgroundMode, Tracker};

fn main() {
    let builder = tauri::Builder::default();
    // The plugin must run before Tauri invokes setup, where the database is opened.
    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
        let existing = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(window) = existing.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        });
    }));

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let data_dir = directories::ProjectDirs::from("", "", "traceTime")
                .ok_or("cannot resolve data dir")?
                .data_dir()
                .to_path_buf();
            std::fs::create_dir_all(&data_dir)?;
            // Also guard the DB if platform instance discovery races or is unavailable.
            let Some(lock) = store::try_lock_instance(&data_dir.join("traceTime.lock"))? else {
                app.handle().exit(0);
                return Ok(());
            };
            app.manage(lock);
            let tracker = Tracker::new(store::open(&data_dir.join("traceTime.db"))?);
            tracker.recover_running_tasks(Utc::now())?;
            let window = app
                .get_webview_window("main")
                .ok_or("main window not found")?;
            let restored = tracker
                .widget_position()
                .and_then(|saved| position::restore(&window, saved).map_err(|e| e.to_string()));
            if let Err(error) = restored {
                eprintln!("cannot restore widget position: {error}");
            }
            let background_mode = tracker.background_mode().unwrap_or(BackgroundMode::Glass);
            let background_opacity = tracker
                .background_opacity()
                .unwrap_or(crate::tracker::DEFAULT_BACKGROUND_OPACITY);
            app.manage(tracker);
            background::initialize_widget_background(
                app,
                &window,
                background_mode,
                background_opacity,
            );

            let app_handle = app.handle().clone();
            app.manage(position::PositionWriter::start(move |x, y| {
                if let Err(error) = app_handle.state::<Tracker>().set_widget_position(x, y) {
                    eprintln!("cannot save widget position: {error}");
                }
            })?);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main"
                && let tauri::WindowEvent::Moved(position) = event
                && let Some(writer) = window.try_state::<position::PositionWriter>()
                && let Err(error) = writer.queue((position.x, position.y))
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
            background::apply_background_mode,
            background::get_widget_background_mode,
            background::set_widget_idle_fade,
            background::set_idle_fade_opacity,
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
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit)
                && let Some(writer) = app.try_state::<position::PositionWriter>()
            {
                writer.flush_and_join();
            }
        });
}
