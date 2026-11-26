import assert from 'node:assert/strict'
import { test } from 'node:test'

test('reminderLevel maps progressive thresholds and overdue', async () => {
  const { reminderLevel } = await import('../src/lib/reminder.ts')
  const config = { reminderEnabled: true, warnPercent: 75, nearPercent: 90, duePercent: 100, overdueIntervalMinutes: 10 }
  assert.equal(reminderLevel(0, 60, config), 'none')
  assert.equal(reminderLevel(45, 60, config), 'warn')
  assert.equal(reminderLevel(55, 60, config), 'near')
  assert.equal(reminderLevel(60, 60, config), 'due')
  assert.equal(reminderLevel(61, 60, config), 'overdue')
  assert.equal(reminderLevel(50, 60, { ...config, reminderEnabled: false }), 'none')
})

test('reminderPulse keeps transient animation away from steady colors', async () => {
  const { reminderPulse } = await import('../src/lib/reminder.ts')
  assert.equal(reminderPulse('none'), 'none')
  assert.equal(reminderPulse('warn'), 'gentle')
  assert.equal(reminderPulse('due'), 'strong')
  assert.equal(reminderPulse('overdue'), 'none')
})

test('reminderPulseEvent gives equal-strength events distinct retrigger identities', async () => {
  const { reminderPulse, reminderPulseEvent } = await import('../src/lib/reminder.ts')
  assert.equal(typeof reminderPulseEvent, 'function')
  const near = reminderPulseEvent('task-A', 'near', 'near', 1000)
  const due = reminderPulseEvent('task-A', 'due', 'due', 1200)
  assert.equal(reminderPulse(near.level), 'strong')
  assert.equal(reminderPulse(due.level), 'strong')
  assert.notEqual(near.key, due.key)
  assert.deepEqual(due, { level: 'due', until: 4200, key: 'task-A:due' })
})
