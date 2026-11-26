import { save } from '@tauri-apps/plugin-dialog'
import { writeTextFile } from '@tauri-apps/plugin-fs'
import type { TaskDetail } from './api'
import { buildTaskExport, exportFileName } from './history'

/** Opens the user-selected save path; cancellation is a normal false result. */
export async function exportTaskDetail(detail: TaskDetail): Promise<boolean> {
  const path = await save({
    defaultPath: exportFileName(detail.task),
    filters: [{ name: 'traceTime JSON', extensions: ['json'] }],
  })
  if (!path) return false
  await writeTextFile(path, JSON.stringify(buildTaskExport(detail), null, 2))
  return true
}
