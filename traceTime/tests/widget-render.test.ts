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
  paused_since: new Date(nowTick - 60_000).toISOString() }
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
