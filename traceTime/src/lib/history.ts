import type { Event as TraceEvent, Segment, Task, TaskDetail } from './api'

/** Derived display data only: never persist pause intervals or mutate the detail DTO. */
export function historyTimeline(detail: TaskDetail | null): {
  segments: Segment[]; pauseCount: number; pauseSeconds: number
} {
  if (!detail) return { segments: [], pauseCount: 0, pauseSeconds: 0 }
  const pauses: Segment[] = []
  let pauseCount = 0
  let pauseSeconds = 0
  // task_detail returns append order (events.id), not wall-clock order. Never sort events by at.
  for (const [index, event] of detail.events.entries()) {
    if (event.kind !== 'paused') continue
    pauseCount++
    const closing = detail.events.slice(index + 1).find(e =>
      e.kind === 'resumed' || e.kind === 'finished' || e.kind === 'abandoned')
    const endedAt = closing?.at ?? (
      detail.task.status === 'done' || detail.task.status === 'abandoned'
        ? detail.task.finished_at : null)
    // An ongoing pause has no closed display interval.
    if (!endedAt) continue
    const start = Date.parse(event.at)
    const end = Date.parse(endedAt)
    if (!Number.isFinite(start) || !Number.isFinite(end)) continue
    pauseSeconds += Math.max(0, (end - start) / 1000)
    pauses.push({
      started_at: event.at,
      ended_at: end < start ? event.at : endedAt,
      kind: 'pause',
    })
  }
  const segments = [...detail.segments, ...pauses]
    .sort((a, b) => Date.parse(a.started_at) - Date.parse(b.started_at))
  return { segments, pauseCount, pauseSeconds }
}

/** Same clock-rollback semantics as Rust: elapsed time cannot be negative. */
export function segmentSeconds(segment: Segment): number {
  const seconds = (Date.parse(segment.ended_at) - Date.parse(segment.started_at)) / 1000
  return Number.isFinite(seconds) ? Math.max(0, seconds) : 0
}

export function wallClockSeconds(task: Task, segments: Segment[]): number {
  const ends = segments.map(s => Date.parse(s.ended_at)).filter(Number.isFinite)
  const end = task.finished_at !== null ? Date.parse(task.finished_at)
    : ends.length ? Math.max(...ends) : Date.parse(task.created_at)
  const seconds = (end - Date.parse(task.created_at)) / 1000
  return Number.isFinite(seconds) ? Math.max(0, seconds) : 0
}

export function durationPercent(seconds: number, wallSeconds: number): number {
  if (!Number.isFinite(seconds) || !Number.isFinite(wallSeconds) || wallSeconds <= 0) return 0
  return Math.round(Math.max(0, seconds) / wallSeconds * 100)
}

export interface TaskExport {
  version: number
  task: Task
  segments: Segment[]
  events: TraceEvent[]
  pause_intervals: Segment[]
}

export function buildTaskExport(detail: TaskDetail): TaskExport {
  return {
    version: 1,
    task: detail.task,
    segments: detail.segments,
    events: detail.events,
    pause_intervals: historyTimeline(detail).segments.filter(segment => segment.kind === 'pause'),
  }
}

export function exportFileName(task: Task): string {
  return `tracetime-${encodeURIComponent(task.name)}-${task.id.slice(0, 8)}.json`
}
