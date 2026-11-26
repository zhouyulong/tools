import assert from 'node:assert/strict'
import { test } from 'node:test'
import { createWidgetHeightObserver } from '../src/lib/widgetHeight.ts'

test('widget height reads unclipped scrollHeight, debounces, deduplicates, and disconnects', async () => {
  let notify = () => {}
  let disconnected = false
  let observed: unknown
  class Observer {
    constructor(callback: () => void) { notify = callback }
    observe(target: unknown) { observed = target }
    disconnect() { disconnected = true }
  }
  const size = { clientHeight: 80, offsetHeight: 95, scrollHeight: 145 }
  const calls: number[] = []
  const { measureNow, cleanup } = createWidgetHeightObserver(
    size as HTMLElement, async height => { calls.push(height) }, Observer, 5)
  assert.equal(observed, size)
  measureNow()
  assert.deepEqual(calls, [145])
  size.scrollHeight = 180
  notify()
  await new Promise(resolve => setTimeout(resolve, 20))
  assert.deepEqual(calls, [145, 180])
  notify()
  await new Promise(resolve => setTimeout(resolve, 20))
  assert.deepEqual(calls, [145, 180])
  size.scrollHeight = 190
  notify()
  cleanup()
  await new Promise(resolve => setTimeout(resolve, 20))
  assert.deepEqual(calls, [145, 180])
  assert.equal(disconnected, true)
})

test('widget resize requests use a 240 ceiling and ignore equivalent clamped heights', async () => {
  let notify = () => {}
  class Observer {
    constructor(callback: () => void) { notify = callback }
    observe() {}
    disconnect() {}
  }
  const size = { clientHeight: 100, offsetHeight: 100, scrollHeight: 100 }
  const calls: number[] = []
  const { measureNow, cleanup } = createWidgetHeightObserver(
    size as HTMLElement, async height => { calls.push(height) }, Observer, 0)
  const flush = () => new Promise(resolve => setTimeout(resolve, 10))
  await flush()
  size.scrollHeight = 130
  notify()
  await flush()
  size.scrollHeight = 300
  notify()
  await flush()
  size.scrollHeight = 240
  measureNow()
  assert.deepEqual(calls, [140, 240])
  cleanup()
})
