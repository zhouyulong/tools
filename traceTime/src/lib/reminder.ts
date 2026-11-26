import type { ReminderConfig } from './api'

export type ReminderLevel = 'none' | 'warn' | 'near' | 'due' | 'overdue'
export type ReminderPulse = 'none' | 'gentle' | 'strong'

type VisualReminderConfig = Pick<
  ReminderConfig,
  'reminderEnabled' | 'warnPercent' | 'nearPercent' | 'duePercent' | 'overdueIntervalMinutes'
> & Partial<Pick<ReminderConfig, 'warnEnabled' | 'nearEnabled' | 'dueEnabled' | 'overdueEnabled'>>

export function reminderLevel(
  netSeconds: number,
  estSeconds: number,
  config: VisualReminderConfig,
): ReminderLevel {
  if (!config.reminderEnabled || estSeconds <= 0) return 'none'

  const elapsed = Math.max(0, netSeconds)
  const threshold = (percent: number) => estSeconds * percent / 100
  const dueAt = threshold(config.duePercent)

  if (config.overdueEnabled !== false && elapsed > dueAt) return 'overdue'
  if (config.dueEnabled !== false && elapsed >= dueAt) return 'due'
  if (config.nearEnabled !== false && elapsed >= threshold(config.nearPercent)) return 'near'
  if (config.warnEnabled !== false && elapsed >= threshold(config.warnPercent)) return 'warn'
  return 'none'
}

export function reminderPulse(activeLevel: ReminderLevel): ReminderPulse {
  if (activeLevel === 'warn') return 'gentle'
  if (activeLevel === 'near' || activeLevel === 'due') return 'strong'
  return 'none'
}

let audioContext: AudioContext | null = null

function chimeSegment(context: AudioContext, frequency: number, start: number, duration: number) {
  const oscillator = context.createOscillator()
  const gain = context.createGain()
  oscillator.type = 'sine'
  oscillator.frequency.setValueAtTime(frequency, start)
  gain.gain.setValueAtTime(0.0001, start)
  gain.gain.exponentialRampToValueAtTime(0.12, start + 0.015)
  gain.gain.exponentialRampToValueAtTime(0.0001, start + duration)
  oscillator.connect(gain)
  gain.connect(context.destination)
  oscillator.start(start)
  oscillator.stop(start + duration)
}

export async function playReminderChime(kind: 'soft' | 'urgent'): Promise<void> {
  try {
    audioContext ??= new AudioContext()
    if (audioContext.state === 'suspended') await audioContext.resume()
    const now = audioContext.currentTime
    if (kind === 'soft') {
      chimeSegment(audioContext, 660, now, 0.16)
    } else {
      chimeSegment(audioContext, 880, now, 0.11)
      chimeSegment(audioContext, 880, now + 0.16, 0.11)
    }
  } catch {
    // Browsers may block audio until the user interacts; reminders must keep working silently.
  }
}
