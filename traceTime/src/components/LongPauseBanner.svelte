<script lang="ts">
  import { abandonTask, finishTask, resume, type Task } from '../lib/api'
  import { bindPausedTask } from '../lib/taskActions'
  import { fmtHMS } from '../lib/format'

  export let task: Task
  export let getCurrentTask: () => Task | null
  export let thresholdHours: number
  export let nowTick: number
  export let refreshAll: () => Promise<void> = async () => {}

  const renderedTaskId = task.id
  const boundTask = bindPausedTask(renderedTaskId, () => getCurrentTask())

  let confirmAbandon = false
  let busy = false
  let error = ''

  $: pausedFor = task.paused_since
    ? Math.max(0, (nowTick - new Date(task.paused_since).getTime()) / 1000)
    : 0
  $: visible = task.status === 'paused' && pausedFor > thresholdHours * 3600

  async function act(action: (expectedTaskId: string) => Promise<Task>) {
    if (busy || task.id !== renderedTaskId || !boundTask.isCurrent()) return
    busy = true
    error = ''
    try {
      const result = await boundTask.run(action)
      if (result) await refreshAll()
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }

  function doFinish() {
    void act(finishTask)
  }

  function doAbandon() {
    if (busy || task.id !== renderedTaskId || !boundTask.isCurrent()) return
    if (!confirmAbandon) {
      confirmAbandon = true
      return
    }
    void act(abandonTask)
  }
</script>

{#if visible}
  <div class="banner" role="status">
    <div class="message" title={`任务 ${task.name} 已暂停 ${fmtHMS(pausedFor)}`}>任务 {task.name} 已暂停 {fmtHMS(pausedFor)}——继续 / 完成 / 放弃？</div>
    <div class="actions">
      <button disabled={busy} on:click={() => act(resume)}>继续</button>
      <button disabled={busy} on:click={doFinish}>完成</button>
      <button disabled={busy} class:confirm={confirmAbandon} on:click={doAbandon}>
        {confirmAbandon ? '确认放弃?' : '放弃'}
      </button>
    </div>
    {#if error}<p class="error" title={error}>{error}</p>{/if}
  </div>
{/if}

<style>
  .banner { flex: none; overflow: hidden; padding: 3px 6px; border-bottom: 1px solid #d69e2e; background: rgba(214, 158, 46, 0.18); }
  .message { height: 12px; line-height: 12px; font-size: 10px; margin-bottom: 2px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .actions { display: flex; gap: 4px; height: 20px; }
  .actions button { flex: 1; cursor: pointer; font-size: 10px; }
  .actions button.confirm { color: #ffb3ad; border-color: #c0392b; }
  .error { color: #ffb3ad; font-size: 10px; height: 12px; line-height: 12px; margin: 2px 0 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
