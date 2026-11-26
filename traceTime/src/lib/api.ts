import { invoke } from '@tauri-apps/api/core'

export type BackgroundMode = 'glass' | 'translucent' | 'opaque'
export interface BackgroundModeResult {
  requested: BackgroundMode
  effective: BackgroundMode
}

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
  reminder_muted: boolean
}

export interface ReminderConfig {
  reminderEnabled: boolean
  warnEnabled: boolean
  warnPercent: number
  nearEnabled: boolean
  nearPercent: number
  dueEnabled: boolean
  duePercent: number
  overdueEnabled: boolean
  overdueNotify: boolean
  overdueIntervalMinutes: number
  warnSound: boolean
  nearSound: boolean
  dueNotify: boolean
}

export interface Reminder {
  node: string
  level: 'warn' | 'near' | 'due' | 'overdue'
  title: string
  body: string
  notify: boolean
  sound: boolean
}

export interface ReminderCheck {
  netSeconds: number
  config: ReminderConfig
  reminders: Reminder[]
}

export interface Segment {
  started_at: string
  ended_at: string
  kind: string
}

export interface Event {
  id: number
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
export const resume = (expectedTaskId: string): Promise<Task> => invoke<Task>('resume', { expectedTaskId })
export const finishTask = (expectedTaskId: string): Promise<Task> => invoke<Task>('finish_task', { expectedTaskId })
export const abandonTask = (expectedTaskId: string): Promise<Task> => invoke<Task>('abandon_task', { expectedTaskId })
export const currentTask = () => invoke<Task | null>('current_task')
export const listFinished = () => invoke<Task[]>('list_finished')
export const openHistory = () => invoke<void>('open_history')
export const openSettings = () => invoke<void>('open_settings')

export const listPaused = () => invoke<Task[]>('list_paused')
export const taskDetail = (id: string) => invoke<TaskDetail>('task_detail', { taskId: id })
export const getSetting = (key: string) => invoke<string | null>('get_setting', { key })
export const setSetting = (key: string, value: string) =>
  invoke<void>('set_setting', { key, value })
export const saveSettings = (values: Record<string, string>) =>
  invoke<void>('set_settings', { values })
export const applyBackgroundMode = (mode: BackgroundMode) =>
  invoke<BackgroundModeResult>('apply_background_mode', { mode })

export const setWidgetHeight = (height: number) => invoke<void>('set_widget_height', { height })
export const deleteTask = (taskId: string) => invoke<void>('delete_task', { taskId })
export const deleteTasks = (ids: string[]) => invoke<void>('delete_tasks', { ids })


export const checkReminders = (expectedTaskId: string) =>
  invoke<ReminderCheck>('check_reminders', { expectedTaskId })
export const setReminderMuted = (expectedTaskId: string, muted: boolean) =>
  invoke<Task>('set_reminder_muted', { expectedTaskId, muted })
export const sendReminderNotification = (taskId: string, node: string, level: Reminder['level'], title: string, body: string) =>
  invoke<void>('send_reminder_notification', { taskId, node, level, title, body })
