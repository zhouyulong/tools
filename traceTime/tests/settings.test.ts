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

test('background mode defaults, normalization, and shell classes are explicit', async () => {
  const {
    DEFAULT_BACKGROUND_MODE,
    backgroundClass,
    normalizeBackgroundMode,
  } = await import('../src/lib/settings.ts')

  assert.equal(DEFAULT_BACKGROUND_MODE, 'glass')
  assert.equal(normalizeBackgroundMode(undefined), 'glass')
  assert.equal(normalizeBackgroundMode('translucent'), 'translucent')
  assert.equal(normalizeBackgroundMode('invalid'), 'glass')
  assert.equal(backgroundClass('glass'), 'background-glass')
  assert.equal(backgroundClass('translucent'), 'background-translucent')
  assert.equal(backgroundClass('opaque'), 'background-opaque')
})

test('settings normalization rejects an invalid background without partially accepting other edits', async () => {
  const { normalizeSettings } = await import('../src/lib/settings.ts')
  const result = normalizeSettings({
    background_mode: 'transparent',
    reminder_warn_percent: '90',
  })

  assert.match(result.errors[0], /背景/)
  assert.equal(result.values.background_mode, 'glass')
  assert.equal(result.values.reminder_warn_percent, '75')
})

test('loading keeps valid stored fields and defaults only the malformed fields', async () => {
  const { loadStoredSettings, normalizeSettings } = await import('../src/lib/settings.ts')
  const stored: Record<string, string> = {
    reminder_warn_percent: '82', reminder_near_percent: 'nope', reminder_due_percent: '111',
    reminder_due_notify: 'false', reminder_warn_sound: 'true',
    reminder_overdue_interval_minutes: '0', background_mode: 'opaque',
  }
  const { values, hasInvalidValues, readFailed } = await loadStoredSettings(async key => stored[key] ?? null)
  assert.equal(values.reminder_warn_percent, '82')
  assert.equal(values.reminder_near_percent, '90')
  assert.equal(values.reminder_due_percent, '111')
  assert.equal(values.reminder_due_notify, 'false')
  assert.equal(values.reminder_warn_sound, 'true')
  assert.equal(values.reminder_overdue_interval_minutes, '10')
  assert.equal(values.background_mode, 'opaque')
  assert.equal(hasInvalidValues, true)
  assert.equal(readFailed, false)
  assert.deepEqual(normalizeSettings(values), { values, errors: [] })
})

test('a failed read does not discard unrelated successful reads', async () => {
  const { loadStoredSettings } = await import('../src/lib/settings.ts')
  const { values, hasInvalidValues, readFailed } = await loadStoredSettings(async key => {
    if (key === 'reminder_warn_sound') throw new Error('unavailable')
    if (key === 'reminder_warn_percent') return '95'
    if (key === 'background_mode') return 'not-a-mode'
    return null
  })
  assert.equal(readFailed, true)
  assert.equal(hasInvalidValues, true)
  assert.equal(values.reminder_warn_sound, 'false')
  assert.equal(values.reminder_warn_percent, '95', 'preserve the stored field on load; sort only on save')
  assert.equal(values.background_mode, 'glass')
})

test('background opacity defaults, normalizes, and rejects out-of-range values', async () => {
  const {
    DEFAULT_BACKGROUND_OPACITY,
    normalizeSettings,
  } = await import('../src/lib/settings.ts')

  assert.equal(DEFAULT_BACKGROUND_OPACITY, 78)
  const valid = normalizeSettings({ background_mode: 'translucent', background_opacity: '65' })
  assert.equal(valid.values.background_opacity, '65')
  assert.deepEqual(valid.errors, [])

  const invalid = normalizeSettings({ background_mode: 'translucent', background_opacity: '39' })
  assert.equal(invalid.values.background_opacity, '78')
  assert.match(invalid.errors[0], /透明度/)
})

test('loading preserves a valid opacity and defaults only a malformed opacity', async () => {
  const { loadStoredSettings } = await import('../src/lib/settings.ts')
  const first = await loadStoredSettings(async key => key === 'background_opacity' ? '90' : null)
  assert.equal(first.values.background_opacity, '90')
  assert.equal(first.hasInvalidValues, false)

  const second = await loadStoredSettings(async key => key === 'background_opacity' ? 'yes' : null)
  assert.equal(second.values.background_opacity, '78')
  assert.equal(second.hasInvalidValues, true)
})


test('idle fade opacity defaults, normalizes, and rejects out-of-range values', async () => {
  const {
    DEFAULT_IDLE_FADE_OPACITY,
    normalizeIdleFadeOpacity,
    normalizeSettings,
  } = await import('../src/lib/settings.ts')

  assert.equal(DEFAULT_IDLE_FADE_OPACITY, 20)
  assert.equal(normalizeIdleFadeOpacity(undefined), 20)
  assert.equal(normalizeIdleFadeOpacity('60'), 60)
  assert.equal(normalizeIdleFadeOpacity('91'), 20)

  const valid = normalizeSettings({ idle_fade_opacity: '80' })
  assert.equal(valid.values.idle_fade_opacity, '80')
  assert.deepEqual(valid.errors, [])

  const invalid = normalizeSettings({ idle_fade_opacity: '91' })
  assert.equal(invalid.values.idle_fade_opacity, '20')
  assert.match(invalid.errors[0], /闲置不透明度/)
})

test('loading preserves a valid idle fade opacity and defaults only a malformed value', async () => {
  const { loadStoredSettings } = await import('../src/lib/settings.ts')
  const first = await loadStoredSettings(async key => key === 'idle_fade_opacity' ? '60' : null)
  assert.equal(first.values.idle_fade_opacity, '60')
  assert.equal(first.hasInvalidValues, false)

  const second = await loadStoredSettings(async key => key === 'idle_fade_opacity' ? '91' : null)
  assert.equal(second.values.idle_fade_opacity, '20')
  assert.equal(second.hasInvalidValues, true)
})
