import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'
import { compile } from 'svelte/compiler'
import { render } from 'svelte/server'
import type { Task } from '../src/lib/api.ts'

// Compile the actual Svelte components, without introducing a second build/test framework.
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith('.') && !/\.[a-z]+$/i.test(specifier)) {
      return nextResolve(`${specifier}.ts`, context)
    }
    return nextResolve(specifier, context)
  },
  load(url, context, nextLoad) {
    if (url.endsWith('.svelte')) {
      return { format: 'module', shortCircuit: true,
        source: compile(readFileSync(new URL(url), 'utf8'), { filename: new URL(url).pathname, generate: 'server' }).js.code }
    }
    return nextLoad(url, context)
  },
})
const WidgetRunning = (await import('../src/components/WidgetRunning.svelte')).default
const TimelineBar = (await import('../src/components/TimelineBar.svelte')).default
const nowTick = Date.UTC(2026, 10, 26, 2)
const task: Task = { id: 'B', name: 'Current', est_seconds: 60, status: 'paused', tag: null,
  net_seconds: 0, created_at: new Date(nowTick - 60_000).toISOString(), finished_at: null,
  paused_since: new Date(nowTick - 60_000).toISOString(), reminder_muted: false }
const parent: Task = { ...task, id: 'A', name: 'Parent', paused_since: new Date(nowTick - 3_600_000).toISOString() }

test('paused view exposes only other paused task chips and their elapsed pause', () => {
  const { body } = render(WidgetRunning, { props: { task, paused: [task, parent], nowTick } })
  assert.match(body, /Parent · 已暂停 01:00:00/)
  assert.doesNotMatch(body, /Current · 已暂停/)
  assert.doesNotMatch(body, /继续最近暂停的任务/)
})

test('single paused task and running view do not show paused chips', () => {
  for (const props of [{ task, paused: [task], nowTick },
    { task: { ...task, status: 'running' }, paused: [parent, task], nowTick }]) {
    assert.doesNotMatch(render(WidgetRunning, { props }).body, /Parent · 已暂停|Current · 已暂停/)
  }
})

test('timeline bounds include a trailing pause, not just work intervals', () => {
  const { body } = render(TimelineBar, { props: { segments: [
    { kind: 'work', started_at: '2026-11-26T00:00:00Z', ended_at: '2026-11-26T00:00:10Z' },
    { kind: 'pause', started_at: '2026-11-26T00:00:10Z', ended_at: '2026-11-26T00:00:40Z' },
  ] } })
  assert.match(body, /left: 0%; width: 25%/)
  assert.match(body, /left: 25%; width: 75%/)
})

test('zero-duration rollback pause is not painted as a positive-width interval over work', () => {
  const { body } = render(TimelineBar, { props: { segments: [
    { kind: 'work', started_at: '2026-11-26T00:00:10Z', ended_at: '2026-11-26T00:00:40Z' },
    { kind: 'pause', started_at: '2026-11-26T00:00:20Z', ended_at: '2026-11-26T00:00:20Z' },
  ] } })
  assert.doesNotMatch(body, /title="pause:/)
  assert.match(body, /left: 0%; width: 100%/)
})

test('paused chips expose omitted tasks through a visible +N marker with all names in its title', () => {
  const others = Array.from({ length: 5 }, (_, i) => ({ ...parent, id: `other-${i}`, name: `Other${i + 1}` }))
  const { body } = render(WidgetRunning, { props: { task, paused: [task, ...others], nowTick } })
  // SSR has no measured clientHeight yet: one full row = one task plus the marker.
  assert.match(body, />\+4<\/span>/)
  assert.match(body, /title="Other1\nOther2\nOther3\nOther4\nOther5"/)
  assert.match(body, /Other1 · 已暂停 01:00:00/)
  assert.doesNotMatch(body, /Other[2-5] · 已暂停/)
  assert.doesNotMatch(body, /title="Current\n/)
})

test('timeline tooltips show duration and integer lifetime percentage', () => {
  const { body } = render(TimelineBar, { props: { wallSeconds: 100, segments: [
    { kind: 'work', started_at: '2026-11-26T00:00:10Z', ended_at: '2026-11-26T00:00:40Z' },
  ] } })
  assert.match(body, /00:00:30（占 30%）/)
})


const reminderConfig = {
  reminderEnabled: true, warnEnabled: true, warnPercent: 75, nearEnabled: true, nearPercent: 90,
  dueEnabled: true, duePercent: 95, overdueEnabled: true, overdueNotify: true,
  overdueIntervalMinutes: 10, warnSound: false, nearSound: false, dueNotify: true,
}

function runningWidgetAt(net_seconds: number, config = reminderConfig) {
  return render(WidgetRunning, { props: {
    task: { ...task, status: 'running', est_seconds: 60, net_seconds }, reminderConfig: config,
  } }).body
}

test('an early reminder deadline changes color without ending the actual countdown', () => {
  assert.match(runningWidgetAt(56), /class="widget [^"]*near-state/)
  const due = runningWidgetAt(57)
  assert.match(due, /class="widget [^"]*due-state/)
  assert.match(due, /剩余 00:00:03/)
  assert.doesNotMatch(due, /已到期|超时 \+|静音超时通知/)
})

test('at 150% the mute button and actual overtime precede the red reminder state', () => {
  const config = { ...reminderConfig, duePercent: 150 }
  assert.doesNotMatch(runningWidgetAt(59, config), /静音超时通知/)
  assert.match(runningWidgetAt(60, config), /静音超时通知/)
  const overtime = runningWidgetAt(65, config)
  assert.match(overtime, /超时 \+00:00:05/)
  assert.match(overtime, /class="widget [^"]*near-state/)
  assert.doesNotMatch(overtime, /剩余|已到期|due-state/)
  const due = runningWidgetAt(90, config)
  assert.match(due, /class="widget [^"]*due-state/)
  assert.match(due, /超时 \+00:00:30/)
})

test('disabled reminders still show actual overtime with no reminder color', () => {
  const body = runningWidgetAt(65, { ...reminderConfig, duePercent: 150, reminderEnabled: false })
  assert.match(body, /超时 \+00:00:05/)
  assert.match(body, /静音超时通知/)
  assert.doesNotMatch(body, /near-state|due-state|剩余|已到期/)
})

test('paused view recomputes steady levels from current config with no new pulse', () => {
  const props = { task: { ...task, est_seconds: 61, net_seconds: 58 }, reminderConfig }
  const overdue = render(WidgetRunning, { props }).body
  const near = render(WidgetRunning, { props: { ...props, reminderConfig: { ...reminderConfig, duePercent: 110 } } }).body
  assert.match(overdue, /class="widget [^"]*due-state/)
  assert.match(near, /class="widget [^"]*near-state/)
  assert.doesNotMatch(overdue + near, /pulse-overlay/)
})
