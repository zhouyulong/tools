import assert from 'node:assert/strict'
import { test } from 'node:test'
import { createIdleFadeController } from '../src/lib/idleFade.ts'

type Listener = () => void

class FakeElement {
  style = { opacity: '', transition: '' }
  listeners = new Map<string, Set<Listener>>()

  addEventListener(type: string, listener: Listener) {
    const listeners = this.listeners.get(type) ?? new Set<Listener>()
    listeners.add(listener)
    this.listeners.set(type, listeners)
  }

  removeEventListener(type: string, listener: Listener) {
    this.listeners.get(type)?.delete(listener)
  }

  dispatch(type: string) {
    for (const listener of this.listeners.get(type) ?? []) listener()
  }

  listenerCount() {
    return [...this.listeners.values()].reduce((total, listeners) => total + listeners.size, 0)
  }
}

function createScheduler() {
  let now = 0
  let nextId = 1
  const timers = new Map<number, { due: number; callback: () => void }>()

  return {
    setTimeout(callback: () => void, delay: number) {
      const id = nextId++
      timers.set(id, { due: now + delay, callback })
      return id
    },
    clearTimeout(id: number) {
      timers.delete(id)
    },
    advance(ms: number) {
      now += ms
      while (true) {
        const ready = [...timers.entries()]
          .filter(([, timer]) => timer.due <= now)
          .sort((left, right) => left[1].due - right[1].due)[0]
        if (!ready) return
        timers.delete(ready[0])
        ready[1].callback()
      }
    },
    pendingCount() {
      return timers.size
    },
  }
}

function setup(options: { idleMs?: number; fadeOpacity?: number } = {}) {
  const scheduler = createScheduler()
  const element = new FakeElement()
  const controller = createIdleFadeController({
    ...options,
    schedule: scheduler.setTimeout,
    cancel: scheduler.clearTimeout,
  })
  controller.bind(element as unknown as HTMLElement)
  return { controller, element, scheduler }
}

test('fades to the default opacity after the default ten-second timeout', () => {
  const { element, scheduler } = setup()
  assert.equal(element.style.opacity, '1')
  assert.equal(element.style.transition, 'opacity 300ms ease')

  scheduler.advance(9_999)
  assert.equal(element.style.opacity, '1')
  scheduler.advance(1)
  assert.equal(element.style.opacity, '0.2')
})

test('each supported interaction restores opacity and restarts the idle timeout', () => {
  for (const event of ['mousemove', 'mousedown', 'keydown', 'touchstart']) {
    const { element, scheduler } = setup()
    scheduler.advance(10_000)
    assert.equal(element.style.opacity, '0.2')

    element.dispatch(event)
    assert.equal(element.style.opacity, '1', `${event} should restore opacity`)
    scheduler.advance(9_999)
    assert.equal(element.style.opacity, '1', `${event} should restart the timeout`)
    scheduler.advance(1)
    assert.equal(element.style.opacity, '0.2')
  }
})

test('mouseenter immediately restores a faded widget and restarts the timeout', () => {
  const { element, scheduler } = setup()
  scheduler.advance(10_000)

  element.dispatch('mouseenter')
  assert.equal(element.style.opacity, '1')
  scheduler.advance(10_000)
  assert.equal(element.style.opacity, '0.2')
})

test('non-positive idle time fades immediately with the configured deterministic opacity', () => {
  for (const idleMs of [0, -1]) {
    const { element, scheduler } = setup({ idleMs, fadeOpacity: 0.35 })
    assert.equal(element.style.opacity, '0.35')
    assert.equal(scheduler.pendingCount(), 0)
  }
})

test('reset restores opacity and replaces the pending timer', () => {
  const { controller, element, scheduler } = setup({ fadeOpacity: 0.4 })
  scheduler.advance(7_000)
  controller.reset()
  assert.equal(element.style.opacity, '1')
  assert.equal(scheduler.pendingCount(), 1)

  scheduler.advance(3_000)
  assert.equal(element.style.opacity, '1')
  scheduler.advance(7_000)
  assert.equal(element.style.opacity, '0.4')
})

test('destroy removes every listener and pending timer without later fading', () => {
  const { controller, element, scheduler } = setup()
  assert.equal(element.listenerCount(), 5)
  assert.equal(scheduler.pendingCount(), 1)

  controller.destroy()
  assert.equal(element.listenerCount(), 0)
  assert.equal(scheduler.pendingCount(), 0)
  scheduler.advance(10_000)
  assert.equal(element.style.opacity, '')
})

test('rebinding the same element does not register duplicate listeners', () => {
  const { controller, element, scheduler } = setup()
  controller.bind(element as unknown as HTMLElement)

  assert.equal(element.listenerCount(), 5)
  assert.equal(scheduler.pendingCount(), 1)
})

test('native transition callbacks run only when faded state changes, even after a rejected fade', async () => {
  const scheduler = createScheduler()
  const element = new FakeElement()
  const states: boolean[] = []
  const controller = createIdleFadeController({
    schedule: scheduler.setTimeout,
    cancel: scheduler.clearTimeout,
    onFadedChange: async faded => {
      states.push(faded)
      if (faded) throw new Error('native effect unavailable')
    },
  })
  controller.bind(element as unknown as HTMLElement)
  element.dispatch('mousemove')
  scheduler.advance(10_000)
  assert.equal(element.style.opacity, '0.2')
  await Promise.resolve()
  assert.deepEqual(states, [true])

  element.dispatch('mouseenter')
  element.dispatch('mousemove')
  assert.equal(element.style.opacity, '1')
  await new Promise(resolve => setImmediate(resolve))
  assert.deepEqual(states, [true, false])
})
