import { invoke } from '@tauri-apps/api/core'
import type { TaskDetail } from './api'
import { buildTaskExport, exportFileName } from './history'

/** The backend owns both the dialog and file write; cancellation returns null. */
export async function exportTaskDetail(detail: TaskDetail): Promise<boolean> {
  const path = await invoke<string | null>('save_text_file_via_dialog', {
    contents: JSON.stringify(buildTaskExport(detail), null, 2),
    fileName: exportFileName(detail.task),
  })
  return path !== null
}
