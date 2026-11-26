import { invoke } from '@tauri-apps/api/core'

export interface Task {
  id: string
  name: string
  est_seconds: number
  status: string
  tag: string | null
  net_seconds: number
  created_at: string
  finished_at: string | null
  paused_since: string | null
}

export interface Segment {
  started_at: string
  ended_at: string
  kind: string
}

export interface Event {
  at: string
  kind: string
  detail: string | null
}

export interface TaskDetail {
  task: Task
  segments: Segment[]
  events: Event[]
}

export const PAUSE_REASONS = [
  { value: 'rest', label: '临时离开' },
  { value: 'disturbance', label: '被干扰' },
  { value: 'switch', label: '切换任务' },
  { value: 'other', label: '其他' },
] as const

export const EST_PRESETS = [15, 25, 45, 60, 120] // 分钟
export const TAG_PRESETS = ['科研', '行政', '杂务']

export const createTask = (name: string, estSeconds: number, tag?: string | null, parentTaskId?: string) =>
  invoke<Task>('create_task', {
    name,
    estSeconds: Math.round(estSeconds),
    tag: tag ?? null,
    parentTaskId: parentTaskId ?? null,
  })
export const pause = (reason: string) => invoke<Task>('pause', { reason })
export const resume = (): Promise<Task> => invoke<Task>('resume')
export const finishTask = (): Promise<Task> => invoke<Task>('finish_task')
export const abandonTask = (): Promise<Task> => invoke<Task>('abandon_task')
export const currentTask = () => invoke<Task | null>('current_task')
export const listFinished = () => invoke<Task[]>('list_finished')

export const listPaused = () => invoke<Task[]>('list_paused')
export const taskDetail = (id: string) => invoke<TaskDetail>('task_detail', { taskId: id })
export const getSetting = (key: string) => invoke<string | null>('get_setting', { key })
export const setSetting = (key: string, value: string) =>
  invoke<void>('set_setting', { key, value })
