export const REMINDER_SETTING_KEYS = [
  'reminder_enabled',
  'reminder_warn_enabled',
  'reminder_warn_percent',
  'reminder_warn_sound',
  'reminder_near_enabled',
  'reminder_near_percent',
  'reminder_near_sound',
  'reminder_due_enabled',
  'reminder_due_percent',
  'reminder_due_notify',
  'reminder_overdue_enabled',
  'reminder_overdue_notify',
  'reminder_overdue_interval_minutes',
] as const

export const BACKGROUND_MODES = ['glass', 'translucent', 'opaque'] as const
export const DEFAULT_BACKGROUND_MODE = 'glass' as const
export const BACKGROUND_MODE_KEY = 'background_mode' as const
export const BACKGROUND_OPACITY_KEY = 'background_opacity' as const
export const DEFAULT_BACKGROUND_OPACITY = 78
export const BACKGROUND_OPACITY_MIN = 40
export const BACKGROUND_OPACITY_MAX = 90

export type ReminderSettingKey = typeof REMINDER_SETTING_KEYS[number]
export type ReminderSettingValues = Record<ReminderSettingKey, string>
export type BackgroundMode = typeof BACKGROUND_MODES[number]
export type SettingKey = ReminderSettingKey | typeof BACKGROUND_MODE_KEY | typeof BACKGROUND_OPACITY_KEY
export type SettingValues = ReminderSettingValues & Record<typeof BACKGROUND_MODE_KEY, BackgroundMode> & Record<typeof BACKGROUND_OPACITY_KEY, string>

export type SettingSpec = {
  key: ReminderSettingKey
  label: string
  kind: 'boolean' | 'number'
  default: string
  min?: number
  max?: number
}

export const SETTINGS: readonly SettingSpec[] = [
  { key: 'reminder_enabled', label: '启用提醒', kind: 'boolean', default: 'true' },
  { key: 'reminder_warn_enabled', label: '提前预警视觉提醒', kind: 'boolean', default: 'true' },
  { key: 'reminder_warn_percent', label: '提前预警百分比', kind: 'number', default: '75', min: 1, max: 1000 },
  { key: 'reminder_warn_sound', label: '提前预警声音', kind: 'boolean', default: 'false' },
  { key: 'reminder_near_enabled', label: '临近截止视觉提醒', kind: 'boolean', default: 'true' },
  { key: 'reminder_near_percent', label: '临近截止百分比', kind: 'number', default: '90', min: 1, max: 1000 },
  { key: 'reminder_near_sound', label: '临近截止声音', kind: 'boolean', default: 'false' },
  { key: 'reminder_due_enabled', label: '到期视觉提醒', kind: 'boolean', default: 'true' },
  { key: 'reminder_due_percent', label: '到期百分比', kind: 'number', default: '100', min: 1, max: 1000 },
  { key: 'reminder_due_notify', label: '到期系统通知', kind: 'boolean', default: 'true' },
  { key: 'reminder_overdue_enabled', label: '启用超时提醒', kind: 'boolean', default: 'true' },
  { key: 'reminder_overdue_notify', label: '超时系统通知', kind: 'boolean', default: 'true' },
  { key: 'reminder_overdue_interval_minutes', label: '超时重复周期', kind: 'number', default: '10', min: 1, max: 1440 },
]

const SPEC_BY_KEY = new Map(SETTINGS.map(spec => [spec.key, spec]))

export function defaultReminderSettings(): ReminderSettingValues {
  return Object.fromEntries(SETTINGS.map(spec => [spec.key, spec.default])) as ReminderSettingValues
}

export function defaultSettings(): SettingValues {
  return {
    ...defaultReminderSettings(),
    background_mode: DEFAULT_BACKGROUND_MODE,
    background_opacity: String(DEFAULT_BACKGROUND_OPACITY),
  }
}

export function normalizeBackgroundMode(value: unknown): BackgroundMode {
  const normalized = String(value ?? '').trim()
  return BACKGROUND_MODES.includes(normalized as BackgroundMode)
    ? normalized as BackgroundMode
    : DEFAULT_BACKGROUND_MODE
}

export function normalizeBackgroundOpacity(value: unknown): number {
  const parsed = Number(String(value ?? '').trim())
  return Number.isInteger(parsed)
    && parsed >= BACKGROUND_OPACITY_MIN
    && parsed <= BACKGROUND_OPACITY_MAX
    ? parsed
    : DEFAULT_BACKGROUND_OPACITY
}

export function backgroundClass(mode: BackgroundMode): `background-${BackgroundMode}` {
  return `background-${mode}`
}

function normalizeValue(spec: SettingSpec, raw: string, stored = false): string | undefined {
  const value = raw.trim()
  if (spec.kind === 'boolean') {
    if (stored && (value === '1' || value === '0')) return String(value === '1')
    return value === 'true' || value === 'false' ? value : undefined
  }
  if (stored && !/^[+-]?\d+$/.test(value)) return undefined
  const parsed = Number(value)
  return Number.isInteger(parsed) && parsed >= spec.min! && parsed <= spec.max! ? String(parsed) : undefined
}

// Loading is tolerant and field-local. Only saving sorts thresholds and rejects invalid edits.
export async function loadStoredSettings(get: (key: SettingKey) => Promise<string | null>) {
  const values = defaultSettings()
  let hasInvalidValues = false
  let readFailed = false
  const keys = [...REMINDER_SETTING_KEYS, BACKGROUND_MODE_KEY, BACKGROUND_OPACITY_KEY] as const
  const results = await Promise.allSettled(keys.map(async key => get(key)))
  keys.forEach((key, index) => {
    const result = results[index]
    if (result.status === 'rejected') {
      readFailed = true
      return
    }
    const raw = result.value
    if (raw === null) return
    if (typeof raw !== 'string') {
      hasInvalidValues = true
      return
    }
    const value = raw.trim()
    if (key === BACKGROUND_MODE_KEY) {
      const mode = normalizeBackgroundMode(value)
      if (mode === value) values[key] = mode
      else hasInvalidValues = true
    } else if (key === BACKGROUND_OPACITY_KEY) {
      const opacity = normalizeBackgroundOpacity(value)
      if (String(opacity) === value) values[key] = String(opacity)
      else hasInvalidValues = true
    } else {
      const normalized = normalizeValue(SPEC_BY_KEY.get(key)!, value, true)
      if (normalized === undefined) hasInvalidValues = true
      else values[key] = normalized
    }
  })
  return { values, hasInvalidValues, readFailed }
}

export function normalizeReminderSettings(
  input: Partial<Record<ReminderSettingKey, string>>,
): { values: ReminderSettingValues; errors: string[] } {
  const values = defaultReminderSettings()
  const errors: string[] = []

  for (const key of REMINDER_SETTING_KEYS) {
    const spec = SPEC_BY_KEY.get(key)!
    const raw = input[key]
    if (raw === undefined || raw === null) continue
    const value = normalizeValue(spec, raw)
    if (value !== undefined) values[key] = value
    else if (spec.kind === 'boolean') errors.push(`${spec.label}必须为 true 或 false`)
    else errors.push(`${spec.label}必须是 ${spec.min}–${spec.max} 的整数`)
  }

  if (errors.length === 0) {
    const ordered = [
      Number(values.reminder_warn_percent),
      Number(values.reminder_near_percent),
      Number(values.reminder_due_percent),
    ].sort((a, b) => a - b)
    values.reminder_warn_percent = String(ordered[0])
    values.reminder_near_percent = String(ordered[1])
    values.reminder_due_percent = String(ordered[2])
  }

  return { values, errors }
}

export function normalizeSettings(
  input: Partial<Record<SettingKey, string>>,
): { values: SettingValues; errors: string[] } {
  const reminder = normalizeReminderSettings(input)
  const rawBackground = input.background_mode
  const background = normalizeBackgroundMode(rawBackground)
  const rawOpacity = input.background_opacity
  const opacity = normalizeBackgroundOpacity(rawOpacity)
  const errors = [...reminder.errors]
  if (rawBackground !== undefined && String(rawBackground).trim() !== background) {
    errors.push('背景模式必须为 glass、translucent 或 opaque')
  }
  if (rawOpacity !== undefined && String(opacity) !== String(rawOpacity).trim()) {
    errors.push(`背景透明度必须是 ${BACKGROUND_OPACITY_MIN}–${BACKGROUND_OPACITY_MAX} 的整数`)
  }
  if (errors.length > 0) return { values: defaultSettings(), errors }
  return {
    values: {
      ...reminder.values,
      background_mode: background,
      background_opacity: String(opacity),
    },
    errors,
  }
}
