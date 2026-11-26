<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { currentTask, getSetting, listPaused, setWidgetHeight, type Task } from '../lib/api'
  import { observeWidgetHeight } from '../lib/widgetHeight'
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
    return observeWidgetHeight(main, setWidgetHeight)
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
      on:switching={(event) => (switchingTask = event.detail)}
      on:switched={handleSwitched}
    />
  {:else}
    <TaskForm {refreshAll} />
  {/if}
</main>

<style>
  main { display: flex; flex-direction: column; width: 100%; height: auto; min-height: 140px; overflow: hidden; background: rgba(30, 30, 34, 0.85); color: #f0f0f0; }
</style>
