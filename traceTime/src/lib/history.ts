import type { Segment, TaskDetail } from './api'

/** Derived display data only: never persist pause intervals or mutate the detail DTO. */
export function historyTimeline(detail: TaskDetail | null): {
  segments: Segment[]; pauseCount: number; pauseSeconds: number
} {
  if (!detail) return { segments: [], pauseCount: 0, pauseSeconds: 0 }
  const pauses: Segment[] = []
  let pauseCount = 0
  let pauseSeconds = 0
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
