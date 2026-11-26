import assert from 'node:assert/strict'
import { test } from 'node:test'
import { observeWidgetHeight } from '../src/lib/widgetHeight.ts'

test('widget height reads intrinsic offsetHeight, debounces, deduplicates, and disconnects', async () => {
  let notify = () => {}
  let disconnected = false
  let observed: unknown
  class Observer {
    constructor(callback: () => void) { notify = callback }
    observe(target: unknown) { observed = target }
    disconnect() { disconnected = true }
  }
  const size = { offsetHeight: 145 }
  const element = size as HTMLElement
  const calls: number[] = []
  const cleanup = observeWidgetHeight(element, async height => { calls.push(height) }, Observer, 5)
  assert.equal(observed, element)
  notify()
  size.offsetHeight = 180
  notify()
  await new Promise(resolve => setTimeout(resolve, 20))
  assert.deepEqual(calls, [180])
  notify()
  await new Promise(resolve => setTimeout(resolve, 20))
  assert.deepEqual(calls, [180])
  size.offsetHeight = 190
  notify()
  cleanup()
  await new Promise(resolve => setTimeout(resolve, 20))
  assert.deepEqual(calls, [180])
  assert.equal(disconnected, true)
})


test('widget resize requests clamp to bounds and ignore equivalent clamped heights', async () => {
  let notify = () => {}
  class Observer {
    constructor(callback: () => void) { notify = callback }
    observe() {}
    disconnect() {}
  }
  const size = { offsetHeight: 100 }
  const calls: number[] = []
  const cleanup = observeWidgetHeight(size as HTMLElement, async height => { calls.push(height) }, Observer, 0)
  const flush = () => new Promise(resolve => setTimeout(resolve, 10))
  await flush()
  size.offsetHeight = 130
  notify()
  await flush()
  size.offsetHeight = 300
  notify()
  await flush()
  assert.deepEqual(calls, [140, 220])
  cleanup()
})
