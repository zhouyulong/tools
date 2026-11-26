use crate::tracker::{TaskDetailDto, TaskDto, Tracker};
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};

type R<T> = Result<T, String>;

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
pub fn set_widget_height(app: tauri::AppHandle, height: f64) -> R<()> {
    if !height.is_finite() {
        return Err("invalid widget height".into());
    }
    let window = app
        .get_webview_window("main")
        .ok_or("main window not found")?;
    window
        .set_size(tauri::LogicalSize::new(260.0, height.clamp(140.0, 220.0)))
        .map_err(|e| e.to_string())
}
