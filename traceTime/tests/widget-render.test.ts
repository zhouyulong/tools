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

test('running view changes countdown, border, and mute availability at the configured due second', () => {
  const at = (net_seconds: number) => render(WidgetRunning, { props: {
    task: { ...task, status: 'running', est_seconds: 61, net_seconds }, reminderConfig,
  } }).body
  assert.match(at(56), /剩余 00:00:01/)
  assert.match(at(56), /class="widget [^"]*near-state/)
  assert.doesNotMatch(at(56), /class="mute/)
  assert.match(at(57), /已到期/)
  assert.match(at(57), /class="widget [^"]*due-state/)
  assert.match(at(57), /静音超时通知/)
  assert.match(at(58), /超时 \+00:00:01/)
  assert.match(at(58), /class="widget [^"]*due-state/)
})

test('paused view recomputes steady levels from current config with no new pulse', () => {
  const props = { task: { ...task, est_seconds: 61, net_seconds: 58 }, reminderConfig }
  const overdue = render(WidgetRunning, { props }).body
  const near = render(WidgetRunning, { props: { ...props, reminderConfig: { ...reminderConfig, duePercent: 110 } } }).body
  assert.match(overdue, /class="widget [^"]*due-state/)
  assert.match(near, /class="widget [^"]*near-state/)
  assert.doesNotMatch(overdue + near, /pulse-overlay/)
})
