<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { currentTask, getSetting, listPaused, type Task } from './lib/api'
  import LongPauseBanner from './components/LongPauseBanner.svelte'
  import TaskForm from './components/TaskForm.svelte'
  import WidgetRunning from './components/WidgetRunning.svelte'

  let task: Task | null = null
  let paused: Task[] = []
  let thresholdHours = 2
  let nowTick = Date.now()
  let switchingTask = false
  let interval: ReturnType<typeof setInterval>

  async function refreshAll() {
    nowTick = Date.now()
    const [currentResult, pausedResult] = await Promise.allSettled([
      currentTask(),
      listPaused(),
    ])
    if (currentResult.status === 'fulfilled') task = currentResult.value
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

  onMount(() => {
    void loadLongPauseHours()
    void refreshAll()
    interval = setInterval(refreshAll, 1000)
  })
  onDestroy(() => clearInterval(interval))
</script>

<main>
  {#if task?.status === 'paused'}
    <LongPauseBanner {task} {thresholdHours} {nowTick} {refreshAll} />
  {/if}

  {#if task}
    <WidgetRunning
      {task}
      on:switching={(event) => (switchingTask = event.detail)}
      on:switched={handleSwitched}
    />
  {:else}
    <TaskForm {paused} {nowTick} {refreshAll} />
  {/if}
</main>

<style>
  main { background: rgba(30, 30, 34, 0.85); color: #f0f0f0; min-height: 100vh; }
</style>
