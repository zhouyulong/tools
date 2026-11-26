import { invoke } from '@tauri-apps/api/core'
import type { TaskDetail } from './api'
import { batchExportFileName, buildTaskExport, buildTasksExport, exportFileName } from './history'

/** The backend owns both the dialog and file write; cancellation returns null. */
export async function exportTaskDetail(detail: TaskDetail): Promise<boolean> {
  const path = await invoke<string | null>('save_text_file_via_dialog', {
    contents: JSON.stringify(buildTaskExport(detail), null, 2),
    fileName: exportFileName(detail.task),
  })
  return path !== null
}

/** Batch export remains a JSON array; each item keeps the single-task export shape. */
export async function exportTaskDetails(details: TaskDetail[]): Promise<boolean> {
  const path = await invoke<string | null>('save_text_file_via_dialog', {
    contents: JSON.stringify(buildTasksExport(details), null, 2),
    fileName: batchExportFileName(details.length),
  })
  return path !== null
}
