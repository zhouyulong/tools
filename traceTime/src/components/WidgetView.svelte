<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import {
    checkReminders,
    currentTask,
    getSetting,
    listPaused,
    sendReminderNotification,
    setReminderMuted,
    setWidgetHeight,
    type ReminderConfig,
    type Task,
  } from '../lib/api'
  import { playReminderChime, type ReminderLevel } from '../lib/reminder'
  import { createWidgetHeightObserver } from '../lib/widgetHeight'
  import LongPauseBanner from './LongPauseBanner.svelte'
  import TaskForm from './TaskForm.svelte'
  import WidgetRunning from './WidgetRunning.svelte'

  let main: HTMLElement
  let task: Task | null = null
  let paused: Task[] = []
  let thresholdHours = 2
  let nowTick = Date.now()
  let switchingTask = false
  let interval: ReturnType<typeof setInterval> | undefined
  let measureNow: () => void = () => {}
  let widgetHeight = 0
  let reminderConfig: ReminderConfig | null = null
  let pulse: { level: ReminderLevel; until: number } = { level: 'none', until: 0 }
  let reminderTaskId: string | null = null
  let processedReminderKeys = new Set<string>()

  function resetReminderState(nextTaskId: string | null) {
    if (reminderTaskId === nextTaskId) return
    reminderTaskId = nextTaskId
    reminderConfig = null
    pulse = { level: 'none', until: 0 }
    processedReminderKeys = new Set<string>()
  }

  async function refreshReminders(current: Task) {
    try {
      const result = await checkReminders(current.id)
      if (task?.id !== current.id || reminderTaskId !== current.id) return
      reminderConfig = result.config
      for (const reminder of result.reminders) {
        const key = `${current.id}:${reminder.node}`
        if (processedReminderKeys.has(key)) continue
        processedReminderKeys.add(key)
        pulse = { level: reminder.level, until: Date.now() + 3000 }
        if (reminder.sound) {
          void playReminderChime(reminder.level === 'warn' ? 'soft' : 'urgent')
        }
        if (reminder.notify) {
          void sendReminderNotification(reminder.title, reminder.body).catch(() => {})
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
      if (task?.status === 'running') await refreshReminders(task)
    }
    if (pausedResult.status === 'fulfilled') paused = pausedResult.value
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
    void loadLongPauseHours()
    void refreshAll()
    interval = setInterval(refreshAll, 1000)
    const observer = createWidgetHeightObserver(main, async height => {
      widgetHeight = height
      await setWidgetHeight(height)
    })
    measureNow = observer.measureNow
    return observer.cleanup
  })
  onDestroy(() => {
    if (interval !== undefined) clearInterval(interval)
  })
</script>

<main class="widget-shell" bind:this={main}>
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
</main>

<style>
  main { display: flex; flex-direction: column; width: 100%; height: auto; min-height: 140px; overflow: hidden; background: rgba(30, 30, 34, 0.85); color: #f0f0f0; }
</style>
