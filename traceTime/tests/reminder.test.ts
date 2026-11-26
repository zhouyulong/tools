import assert from 'node:assert/strict'
import { test } from 'node:test'

const config = { reminderEnabled: true, warnPercent: 75, nearPercent: 90, duePercent: 95 }

test('reminder levels floor fractional thresholds like Rust', async () => {
  const { reminderLevel } = await import('../src/lib/reminder.ts')
  assert.equal(reminderLevel(44, 61, config), 'none')
  assert.equal(reminderLevel(45, 61, config), 'warn')
  assert.equal(reminderLevel(54, 61, config), 'near')
  assert.equal(reminderLevel(56, 61, config), 'near')
  assert.equal(reminderLevel(57, 61, config), 'due')
  assert.equal(reminderLevel(58, 61, config), 'overdue')
  assert.equal(reminderLevel(58, 61, { ...config, reminderEnabled: false }), 'none')
})

test('countdown crosses the configured and floored due boundary, not the raw estimate', async () => {
  const { reminderCountdown } = await import('../src/lib/reminder.ts')
  assert.deepEqual(reminderCountdown(56, 61, 95), { phase: 'remaining', seconds: 1, percent: 98 })
  assert.deepEqual(reminderCountdown(57, 61, 95), { phase: 'due', seconds: 0, percent: 100 })
  assert.deepEqual(reminderCountdown(58, 61, 95), { phase: 'overdue', seconds: 1, percent: 100 })
  assert.deepEqual(reminderCountdown(62, 61, 150), { phase: 'remaining', seconds: 29, percent: 68 })
})

test('pulse queue gives every event in a batch its own three-second slot', async ({ mock }) => {
  const { createReminderPulseQueue } = await import('../src/lib/reminder.ts')
  mock.timers.enable({ apis: ['setTimeout'] })
  const shown: (string | null)[] = []
  const queue = createReminderPulseQueue(pulse => shown.push(pulse?.node ?? null))
  for (const [node, level] of [['warn', 'warn'], ['near', 'near'], ['due', 'due'], ['overdue:at:600', 'overdue']] as const) {
    queue.enqueue({ node, level })
  }
  for (const [i, node] of ['warn', 'near', 'due', 'overdue:at:600'].entries()) {
    assert.equal(shown.at(-1), node)
    mock.timers.tick(2999)
    assert.equal(shown.length, i + 1)
    mock.timers.tick(1)
  }
  assert.deepEqual(shown, ['warn', 'near', 'due', 'overdue:at:600', null])
})

test('reset cancels the pending pulses so a new task can animate immediately', async ({ mock }) => {
  const { createReminderPulseQueue } = await import('../src/lib/reminder.ts')
  mock.timers.enable({ apis: ['setTimeout'] })
  const shown: (string | null)[] = []
  const queue = createReminderPulseQueue(pulse => shown.push(pulse?.node ?? null))
  queue.enqueue({ node: 'warn', level: 'warn' })
  queue.enqueue({ node: 'near', level: 'near' })
  mock.timers.tick(500)
  queue.clear()
  queue.enqueue({ node: 'due', level: 'due' })
  mock.timers.tick(2999)
  assert.deepEqual(shown, ['warn', null, 'due'])
  mock.timers.tick(1)
  assert.deepEqual(shown, ['warn', null, 'due', null])
})
