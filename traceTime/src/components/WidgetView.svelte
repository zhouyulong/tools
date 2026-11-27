<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import {
    checkReminders,
    currentTask,
    getSetting,
    getWidgetBackgroundMode,
    listPaused,
    sendReminderNotification,
    setReminderMuted,
    setWidgetHeight,
    setWidgetIdleFade,
    type BackgroundModeResult,
    type ReminderConfig,
    type Task,
  } from '../lib/api'
  import { createReminderPulseQueue, playReminderChime, type ReminderPulse } from '../lib/reminder'
  import { createIdleFadeController } from '../lib/idleFade'
  import { createWidgetHeightObserver } from '../lib/widgetHeight'
  import { backgroundClass, DEFAULT_BACKGROUND_OPACITY, type BackgroundMode } from '../lib/settings'
  import { shouldStartWidgetDrag } from '../lib/widget-drag'
  import LongPauseBanner from './LongPauseBanner.svelte'
  import TaskForm from './TaskForm.svelte'
  import WidgetRunning from './WidgetRunning.svelte'

  let shell: HTMLElement
  let task: Task | null = null
  let paused: Task[] = []
  let thresholdHours = 2
  let nowTick = Date.now()
  let switchingTask = false
  let interval: ReturnType<typeof setInterval> | undefined
  let measureNow: () => void = () => {}
  let widgetHeight = 0
  let reminderConfig: ReminderConfig | null = null
  let pulse: ReminderPulse | null = null
  const pulses = createReminderPulseQueue(next => { pulse = next })
  const idleFade = createIdleFadeController({ onFadedChange: setWidgetIdleFade })
  let reminderTaskId: string | null = null
  let processedReminderKeys = new Set<string>()
  let effectiveBackground: BackgroundMode = 'translucent'
  let backgroundOpacity = DEFAULT_BACKGROUND_OPACITY
  let stopBackgroundListener: (() => void) | undefined
  let destroyed = false

  function resetReminderState(nextTaskId: string | null) {
    if (reminderTaskId === nextTaskId) return
    reminderTaskId = nextTaskId
    pulses.clear()
    processedReminderKeys = new Set<string>()
  }

  async function refreshReminders(current: Task) {
    try {
      const result = await checkReminders(current.id)
      if (task?.id !== current.id || reminderTaskId !== current.id) return
      reminderConfig = result.config
      // Use the exact elapsed-second snapshot that produced the reminder candidates.
      if (task === current && task.net_seconds !== result.netSeconds) {
        task = { ...task, net_seconds: result.netSeconds }
      }
      for (const reminder of result.reminders) {
        const key = `${current.id}:${reminder.node}`
        if (processedReminderKeys.has(key)) continue
        processedReminderKeys.add(key)
        pulses.enqueue(reminder)
        if (reminder.sound) {
          void playReminderChime(reminder.level === 'warn' ? 'soft' : 'urgent')
        }
        if (reminder.notify) {
          void sendReminderNotification(current.id, reminder.node, reminder.level, reminder.title, reminder.body).catch(() => {})
        }
      }
    } catch (error) {
      if (String(error).includes('task changed, please refresh')) return
    }
  }

  async function refreshAll() {
    nowTick = Date.now()
    const [currentResult, pausedResult] = await Promise.allSettled([
      currentTask(),
      listPaused(),
    ])
    if (currentResult.status === 'fulfilled') {
      task = currentResult.value
      resetReminderState(task?.id ?? null)
      if (task) await refreshReminders(task)
    }
    if (pausedResult.status === 'fulfilled') paused = pausedResult.value
  }

  function handleDragPress(event: MouseEvent) {
    if (!shouldStartWidgetDrag(event)) return
    event.preventDefault()
    void getCurrentWindow().startDragging().catch(() => {})
  }

  async function loadBackgroundMode() {
    try {
      const result = await getWidgetBackgroundMode()
      effectiveBackground = result.effective
      backgroundOpacity = result.backgroundOpacity
    } catch {
      // Keep the last known result (or the safe startup CSS fallback).
    }
  }

  async function loadLongPauseHours() {
    try {
      const value = await getSetting('long_pause_hours')
      const parsed = Number(value?.trim() || '2')
      thresholdHours = Number.isFinite(parsed) && parsed >= 0 ? parsed : 2
    } catch {
      thresholdHours = 2
    }
  }

  async function handleSwitched() {
    await refreshAll()
    switchingTask = false
  }

  async function handleMuteReminder(expectedTaskId: string) {
    const updated = await setReminderMuted(expectedTaskId, true)
    if (task?.id !== expectedTaskId || updated.id !== expectedTaskId) {
      throw new Error('task changed, please refresh')
    }
    task = updated
    return updated
  }

  onMount(() => {
    void listen<BackgroundModeResult>('background-mode-changed', event => {
      effectiveBackground = event.payload.effective
      backgroundOpacity = event.payload.backgroundOpacity
    }).then(unlisten => {
      if (destroyed) unlisten()
      else stopBackgroundListener = unlisten
    }).catch(() => {}).then(() => {
      if (!destroyed) void loadBackgroundMode()
    })
    void loadLongPauseHours()
    void refreshAll()
    interval = setInterval(refreshAll, 1000)
    idleFade.bind(shell)
    const observer = createWidgetHeightObserver(shell, async height => {
      widgetHeight = height
      await setWidgetHeight(height)
    })
    measureNow = observer.measureNow
    return () => {
      observer.cleanup()
      idleFade.destroy()
    }
  })
  onDestroy(() => {
    destroyed = true
    stopBackgroundListener?.()
    if (interval !== undefined) clearInterval(interval)
    reminderTaskId = null
    pulses.clear()
  })
</script>

<div
    class={`widget-shell ${backgroundClass(effectiveBackground)}`}
    style={`--background-alpha: ${backgroundOpacity / 100}`}
    role="presentation"
    bind:this={shell}
    on:mousedown={handleDragPress}
  >
  <div class="drag-region" aria-hidden="true"></div>
  {#if task?.status === 'paused' && !switchingTask}
    {#key task.id}
      <LongPauseBanner {task} {thresholdHours} {nowTick} {refreshAll} getCurrentTask={() => task} />
    {/key}
  {/if}

  {#if task}
    <WidgetRunning
      {task}
      {paused}
      {nowTick}
      {widgetHeight}
      {reminderConfig}
      {pulse}
      onMuteReminder={handleMuteReminder}
      on:switching={(event) => (switchingTask = event.detail)}
      on:switched={handleSwitched}
      on:layout={() => measureNow()}
    />
  {:else}
    <TaskForm {refreshAll} />
  {/if}
</div>

<style>
  .widget-shell { display: flex; flex-direction: column; width: 100%; height: auto; min-height: 140px; overflow: hidden; color: #f0f0f0; }
  .drag-region { flex: none; width: 100%; height: 8px; cursor: grab; user-select: none; }
  .drag-region:active { cursor: grabbing; }
  .background-glass { background: rgba(24, 24, 28, 0.52); }
  .background-translucent { background: rgba(24, 24, 28, var(--background-alpha, 0.78)); }
  .background-opaque { background: #18181c; }
</style>
