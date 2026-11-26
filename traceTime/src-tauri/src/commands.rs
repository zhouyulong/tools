use crate::reminder::{ReminderCheckDto, ReminderLevel};
use crate::tracker::{BackgroundMode, TaskDetailDto, TaskDto, Tracker, effective_background_mode};
use serde::Serialize;
use tauri::{Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;

type R<T> = Result<T, String>;

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
pub struct BackgroundModeResult {
    pub requested: BackgroundMode,
    pub effective: BackgroundMode,
}

#[cfg(target_os = "macos")]
fn clear_native_background(window: &WebviewWindow) {
    let _ = window_vibrancy::clear_vibrancy(window);
}

#[cfg(target_os = "windows")]
fn clear_native_background(window: &WebviewWindow) {
    let _ = window_vibrancy::clear_mica(window);
    let _ = window_vibrancy::clear_acrylic(window);
    let _ = window_vibrancy::clear_blur(window);
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn clear_native_background(_window: &WebviewWindow) {}

#[cfg(target_os = "macos")]
fn try_native_glass(window: &WebviewWindow) -> bool {
    window_vibrancy::apply_vibrancy(
        window,
        window_vibrancy::NSVisualEffectMaterial::HudWindow,
        Some(window_vibrancy::NSVisualEffectState::Active),
        Some(8.0),
    )
    .is_ok()
}

#[cfg(target_os = "windows")]
fn try_native_glass(window: &WebviewWindow) -> bool {
    window_vibrancy::apply_mica(window, Some(true)).is_ok()
        || window_vibrancy::apply_acrylic(window, Some((24, 24, 28, 190))).is_ok()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn try_native_glass(_window: &WebviewWindow) -> bool {
    // Linux blur is compositor-controlled; window-vibrancy exposes no portable API.
    false
}

fn apply_native_background(window: &WebviewWindow, requested: BackgroundMode) -> bool {
    clear_native_background(window);
    requested == BackgroundMode::Glass && try_native_glass(window)
}

pub fn apply_background_mode_to_window(
    window: &WebviewWindow,
    requested: BackgroundMode,
) -> BackgroundModeResult {
    let result = BackgroundModeResult {
        requested,
        effective: effective_background_mode(requested, apply_native_background(window, requested)),
    };
    let _ = window.emit("background-mode-changed", result);
    result
}

#[tauri::command]
pub async fn apply_background_mode(app: tauri::AppHandle, mode: String) -> R<BackgroundModeResult> {
    let requested = BackgroundMode::parse(&mode)?;
    let window = app
        .get_webview_window("main")
        .ok_or("main window not found")?;
    let native_window = window.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let scheduled = window.run_on_main_thread(move || {
        let result = apply_background_mode_to_window(&native_window, requested);
        let _ = sender.send(result);
    });
    if scheduled.is_err() {
        let result = BackgroundModeResult {
            requested,
            effective: effective_background_mode(requested, false),
        };
        let _ = window.emit("background-mode-changed", result);
        return Ok(result);
    }
    Ok(tauri::async_runtime::spawn_blocking(move || {
        receiver.recv().unwrap_or(BackgroundModeResult {
            requested,
            effective: effective_background_mode(requested, false),
        })
    })
    .await
    .unwrap_or(BackgroundModeResult {
        requested,
        effective: effective_background_mode(requested, false),
    }))
}

#[tauri::command]
pub fn create_task(
    state: State<'_, Tracker>,
    name: String,
    est_seconds: i64,
    tag: Option<String>,
    parent_task_id: Option<String>,
) -> R<TaskDto> {
    state.create_task(
        &name,
        est_seconds,
        tag.as_deref(),
        parent_task_id.as_deref(),
    )
}

#[tauri::command]
pub fn pause(state: State<'_, Tracker>, reason: String) -> R<TaskDto> {
    state.pause(&reason)
}

#[tauri::command]
pub fn resume(state: State<'_, Tracker>, expected_task_id: String) -> R<TaskDto> {
    state.resume(&expected_task_id)
}

#[tauri::command]
pub fn finish_task(state: State<'_, Tracker>, expected_task_id: String) -> R<TaskDto> {
    state.finish(&expected_task_id)
}

#[tauri::command]
pub fn abandon_task(state: State<'_, Tracker>, expected_task_id: String) -> R<TaskDto> {
    state.abandon(&expected_task_id)
}

#[tauri::command]
pub fn current_task(state: State<'_, Tracker>) -> R<Option<TaskDto>> {
    state.current()
}

#[tauri::command]
pub fn list_finished(state: State<'_, Tracker>) -> R<Vec<TaskDto>> {
    state.list_finished()
}

#[tauri::command]
pub fn list_paused(state: State<'_, Tracker>) -> R<Vec<TaskDto>> {
    state.list_paused()
}

#[tauri::command]
pub fn task_detail(state: State<'_, Tracker>, task_id: String) -> R<TaskDetailDto> {
    state.task_detail(&task_id)
}

#[tauri::command]
pub fn get_setting(state: State<'_, Tracker>, key: String) -> R<Option<String>> {
    state.get_setting(&key)
}

#[tauri::command]
pub fn set_setting(state: State<'_, Tracker>, key: String, value: String) -> R<()> {
    state.set_setting(&key, &value)
}

#[tauri::command]
pub fn set_settings(
    state: State<'_, Tracker>,
    values: std::collections::BTreeMap<String, String>,
) -> R<()> {
    state.set_settings(&values)
}

#[tauri::command]
pub fn check_reminders(state: State<'_, Tracker>, expected_task_id: String) -> R<ReminderCheckDto> {
    state.check_reminders(&expected_task_id)
}

#[tauri::command]
pub fn set_reminder_muted(
    state: State<'_, Tracker>,
    expected_task_id: String,
    muted: bool,
) -> R<TaskDto> {
    state.set_reminder_muted(&expected_task_id, muted)
}

#[tauri::command]
pub fn send_reminder_notification(
    app: tauri::AppHandle,
    state: State<'_, Tracker>,
    task_id: String,
    node: String,
    level: ReminderLevel,
    title: String,
    body: String,
) -> R<()> {
    state.with_reminder_notification(&task_id, &node, level, || {
        app.notification()
            .builder()
            .title(title)
            .body(body)
            .show()
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
pub async fn open_settings(app: tauri::AppHandle) -> R<()> {
    if let Some(window) = app.get_webview_window("settings") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    WebviewWindowBuilder::new(
        &app,
        "settings",
        WebviewUrl::App("index.html#settings".into()),
    )
    .title("traceTime 设置")
    .inner_size(360.0, 480.0)
    .resizable(false)
    .always_on_top(true)
    .build()
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_history(app: tauri::AppHandle) -> R<()> {
    if let Some(window) = app.get_webview_window("history") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    WebviewWindowBuilder::new(
        &app,
        "history",
        WebviewUrl::App("index.html#history".into()),
    )
    .title("traceTime 历史")
    .inner_size(480.0, 520.0)
    .resizable(true)
    .always_on_top(true)
    .build()
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_task(state: State<'_, Tracker>, task_id: String) -> R<()> {
    state.delete_task(&task_id)
}

#[tauri::command]
pub fn delete_tasks(state: State<'_, Tracker>, ids: Vec<String>) -> R<()> {
    state.delete_tasks(&ids)
}

#[tauri::command]
pub async fn save_text_file_via_dialog(
    app: tauri::AppHandle,
    contents: String,
    file_name: String,
) -> R<Option<String>> {
    // Dialog and filesystem I/O must stay off the main thread.
    tauri::async_runtime::spawn_blocking(move || {
        let selected = app
            .dialog()
            .file()
            .set_file_name(&file_name)
            .add_filter("traceTime JSON", &["json"])
            .blocking_save_file();
        match selected {
            None => Ok(None),
            Some(path) => {
                let path = path.into_path().map_err(|e| e.to_string())?;
                std::fs::write(&path, contents).map_err(|e| e.to_string())?;
                Ok(Some(path.to_string_lossy().into_owned()))
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

fn validate_widget_position(x: i64, y: i64) -> R<(i32, i32)> {
    let x = i32::try_from(x).map_err(|_| "invalid widget x position".to_string())?;
    let y = i32::try_from(y).map_err(|_| "invalid widget y position".to_string())?;
    Ok((x, y))
}

#[tauri::command]
pub fn set_widget_position(state: State<'_, Tracker>, x: i64, y: i64) -> R<()> {
    let (x, y) = validate_widget_position(x, y)?;
    state.set_widget_position(x, y)
}

#[tauri::command]
pub fn set_widget_height(app: tauri::AppHandle, height: f64) -> R<()> {
    if !height.is_finite() {
        return Err("invalid widget height".into());
    }
    let window = app
        .get_webview_window("main")
        .ok_or("main window not found")?;
    window
        .set_size(tauri::LogicalSize::new(260.0, height.clamp(140.0, 240.0)))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::validate_widget_position;

    #[test]
    fn widget_position_ipc_rejects_values_outside_i32_range() {
        assert_eq!(validate_widget_position(120, -80).unwrap(), (120, -80));
        assert!(validate_widget_position(i64::from(i32::MAX) + 1, 0).is_err());
        assert!(validate_widget_position(0, i64::from(i32::MIN) - 1).is_err());
    }
}
