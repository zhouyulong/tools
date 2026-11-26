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
}

export const PAUSE_REASONS = [
  { value: 'rest', label: '临时离开' },
  { value: 'disturbance', label: '被干扰' },
  { value: 'switch', label: '切换任务' },
  { value: 'other', label: '其他' },
] as const

export const EST_PRESETS = [15, 25, 45, 60, 120] // 分钟
export const TAG_PRESETS = ['科研', '行政', '杂务']

export const createTask = (name: string, estSeconds: number, tag?: string | null) =>
  invoke<Task>('create_task', { name, estSeconds: Math.round(estSeconds), tag: tag ?? null })
export const pause = (reason: string) => invoke<Task>('pause', { reason })
export const resume = (): Promise<Task> => invoke<Task>('resume')
export const finishTask = (): Promise<Task> => invoke<Task>('finish_task')
export const abandonTask = (): Promise<Task> => invoke<Task>('abandon_task')
export const currentTask = () => invoke<Task | null>('current_task')
export const listFinished = () => invoke<Task[]>('list_finished')
