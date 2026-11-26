use crate::tracker::{TaskDetailDto, TaskDto, Tracker};
use tauri::State;

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
pub fn resume(state: State<'_, Tracker>) -> R<TaskDto> {
    state.resume()
}

#[tauri::command]
pub fn finish_task(state: State<'_, Tracker>) -> R<TaskDto> {
    state.finish()
}

#[tauri::command]
pub fn abandon_task(state: State<'_, Tracker>) -> R<TaskDto> {
    state.abandon()
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
