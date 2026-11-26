import type { Reminder, ReminderConfig } from './api'

export type ReminderLevel = 'none' | Reminder['level']
export type ReminderPulse = Pick<Reminder, 'node' | 'level'>
const PULSE_MS = 3000

const thresholdSeconds = (estSeconds: number, percent: number) =>
  Math.floor(estSeconds * percent / 100)

export function taskCountdown(netSeconds: number, estSeconds: number) {
  const elapsed = Math.max(0, netSeconds)
  const delta = elapsed - estSeconds
  return {
    phase: delta < 0 ? 'remaining' : 'overdue',
    seconds: Math.abs(delta),
    percent: delta >= 0 ? 100 : Math.floor(elapsed / estSeconds * 100),
  } as const
}

type VisualReminderConfig = Pick<ReminderConfig, 'reminderEnabled' | 'warnPercent' | 'nearPercent' | 'duePercent'>
  & Partial<Pick<ReminderConfig, 'warnEnabled' | 'nearEnabled' | 'dueEnabled' | 'overdueEnabled'>>

export function reminderLevel(
  netSeconds: number,
  estSeconds: number,
  config: VisualReminderConfig,
): ReminderLevel {
  if (!config.reminderEnabled || estSeconds <= 0) return 'none'

  const elapsed = Math.max(0, netSeconds)
  const dueAt = thresholdSeconds(estSeconds, config.duePercent)
  if (elapsed > dueAt && config.overdueEnabled !== false) return 'overdue'
  if (elapsed >= dueAt && config.dueEnabled !== false) return 'due'
  if (config.nearEnabled !== false && elapsed >= thresholdSeconds(estSeconds, config.nearPercent)) return 'near'
  if (config.warnEnabled !== false && elapsed >= thresholdSeconds(estSeconds, config.warnPercent)) return 'warn'
  return 'none'
}

export function createReminderPulseQueue(show: (pulse: ReminderPulse | null) => void) {
  const pending: ReminderPulse[] = []
  let timer: ReturnType<typeof setTimeout> | undefined

  function showNext() {
    const pulse = pending.shift() ?? null
    timer = pulse ? setTimeout(showNext, PULSE_MS) : undefined
    show(pulse)
  }

  return {
    enqueue(pulse: ReminderPulse) {
      pending.push(pulse)
      if (timer === undefined) showNext()
    },
    clear() {
      clearTimeout(timer)
      timer = undefined
      pending.length = 0
      show(null)
    },
  }
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
