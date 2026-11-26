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

export type ReminderSettingKey = typeof REMINDER_SETTING_KEYS[number]
export type ReminderSettingValues = Record<ReminderSettingKey, string>
export type BackgroundMode = typeof BACKGROUND_MODES[number]
export type SettingKey = ReminderSettingKey | typeof BACKGROUND_MODE_KEY
export type SettingValues = ReminderSettingValues & Record<typeof BACKGROUND_MODE_KEY, BackgroundMode>

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
  return { ...defaultReminderSettings(), background_mode: DEFAULT_BACKGROUND_MODE }
}

export function normalizeBackgroundMode(value: unknown): BackgroundMode {
  const normalized = String(value ?? '').trim()
  return BACKGROUND_MODES.includes(normalized as BackgroundMode)
    ? normalized as BackgroundMode
    : DEFAULT_BACKGROUND_MODE
}

export function backgroundClass(mode: BackgroundMode): `background-${BackgroundMode}` {
  return `background-${mode}`
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
    const value = String(raw).trim()

    if (spec.kind === 'boolean') {
      if (value === 'true' || value === 'false') values[key] = value
      else errors.push(`${spec.label}必须为 true 或 false`)
      continue
    }

    const parsed = Number(value)
    if (!Number.isInteger(parsed) || parsed < spec.min! || parsed > spec.max!) {
      errors.push(`${spec.label}必须是 ${spec.min}–${spec.max} 的整数`)
      continue
    }
    values[key] = String(parsed)
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
  const errors = [...reminder.errors]
  if (rawBackground !== undefined && String(rawBackground).trim() !== background) {
    errors.push('背景模式必须为 glass、translucent 或 opaque')
  }
  if (errors.length > 0) return { values: defaultSettings(), errors }
  return { values: { ...reminder.values, background_mode: background }, errors }
}
