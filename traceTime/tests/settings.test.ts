import assert from 'node:assert/strict'
import test from 'node:test'

test('settings defaults and normalization preserve stage order', async () => {
  const { SETTINGS, normalizeReminderSettings } = await import('../src/lib/settings.ts')
  assert.equal(SETTINGS.find(x => x.key === 'reminder_warn_percent')?.default, '75')
  const result = normalizeReminderSettings({
    reminder_enabled: 'true',
    reminder_warn_percent: '90',
    reminder_near_percent: '75',
    reminder_due_percent: '120',
    reminder_overdue_interval_minutes: '10',
  })
  assert.deepEqual(result.values.reminder_near_percent, '90')
  assert.deepEqual(result.values.reminder_warn_percent, '75')
  assert.deepEqual(result.errors, [])
})

test('invalid numeric values surface errors without saving silently', async () => {
  const { normalizeReminderSettings } = await import('../src/lib/settings.ts')
  const result = normalizeReminderSettings({ reminder_warn_percent: 'abc' })
  assert.equal(result.errors.length, 1)
  assert.match(result.errors[0], /提前预警/)
})
