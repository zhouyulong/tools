import assert from 'node:assert/strict'
import { test } from 'node:test'
import { historyTimeline } from '../src/lib/history.ts'
import type { TaskDetail, Event } from '../src/lib/api.ts'

const at = (s: number) => new Date(Date.UTC(2026, 10, 26, 0, 0, s)).toISOString()
const event = (s: number, kind: string): Event => ({ at: at(s), kind, detail: null })
function detail(events: Event[], status = 'done', finished_at: string | null = at(50)): TaskDetail {
  return {
    task: { id: 'A', name: 'A', est_seconds: 60, status, tag: null, net_seconds: 10,
      created_at: at(0), finished_at, paused_since: null },
    segments: [{ started_at: at(0), ended_at: at(10), kind: 'work' }], events,
  }
}

test('pause intervals pair with the nearest resume/terminal event and merge without mutation', () => {
  const input = detail([event(10, 'paused'), event(12, 'note'), event(20, 'resumed'),
    event(30, 'paused'), event(40, 'finished')])
  const before = structuredClone(input)
  const result = historyTimeline(input)
  assert.equal(result.pauseCount, 2)
  assert.equal(result.pauseSeconds, 20)
  assert.deepEqual(result.segments.map(s => [s.kind, s.started_at, s.ended_at]), [
    ['work', at(0), at(10)], ['pause', at(10), at(20)], ['pause', at(30), at(40)],
  ])
  assert.deepEqual(input, before)
})

test('abandoned event closes a trailing pause', () => {
  const result = historyTimeline(detail([event(10, 'paused'), event(45, 'abandoned')], 'abandoned', null))
  assert.equal(result.pauseSeconds, 35)
  assert.equal(result.segments.at(-1)?.ended_at, at(45))
})

test('terminal task without closing event falls back to finished_at', () => {
  for (const status of ['done', 'abandoned']) {
    const result = historyTimeline(detail([event(10, 'paused')], status))
    assert.equal(result.pauseSeconds, 40)
    assert.equal(result.segments.at(-1)?.ended_at, at(50))
  }
})

test('still-paused task counts the pause but emits no unfinished trailing interval', () => {
  const result = historyTimeline(detail([event(10, 'paused')], 'paused', null))
  assert.equal(result.pauseCount, 1)
  assert.equal(result.pauseSeconds, 0)
  assert.equal(result.segments.length, 1)
})

test('empty detail and clock rollback never yield negative pause duration', () => {
  assert.deepEqual(historyTimeline(null), { segments: [], pauseCount: 0, pauseSeconds: 0 })
  const result = historyTimeline(detail([event(20, 'paused'), event(10, 'resumed')]))
  assert.equal(result.pauseSeconds, 0)
  assert.equal(result.segments.at(-1)?.ended_at, at(20))
})
