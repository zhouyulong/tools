<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { currentTask, type Task } from './lib/api'
  import TaskForm from './components/TaskForm.svelte'
  import WidgetRunning from './components/WidgetRunning.svelte'

  let task: Task | null = null
  let interval: ReturnType<typeof setInterval>

  async function refresh() {
    try { task = await currentTask() } catch { /* 保持上次状态 */ }
  }

  onMount(() => {
    refresh()
    interval = setInterval(refresh, 1000)
  })
  onDestroy(() => clearInterval(interval))
</script>

<main>
  {#if task && task.status !== 'done' && task.status !== 'abandoned'}
    <WidgetRunning {task} />
  {:else}
    <TaskForm />
  {/if}
</main>

<style>
  main { background: rgba(30, 30, 34, 0.85); color: #f0f0f0; min-height: 100vh; }
</style>
