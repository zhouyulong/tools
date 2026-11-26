<script lang="ts">
  import { abandonTask, finishTask, resume, type Task } from '../lib/api'
  import { fmtHMS } from '../lib/format'

  export let task: Task
  export let thresholdHours: number
  export let nowTick: number
  export let refreshAll: () => Promise<void> = async () => {}

  let confirmAbandon = false
  let busy = false
  let error = ''

  $: pausedFor = task.paused_since
    ? Math.max(0, (nowTick - new Date(task.paused_since).getTime()) / 1000)
    : 0
  $: visible = task.status === 'paused' && pausedFor > thresholdHours * 3600

  async function act(action: () => Promise<Task>) {
    if (busy) return
    busy = true
    error = ''
    try {
      await action()
      await refreshAll()
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }

  function doAbandon() {
    if (!confirmAbandon) {
      confirmAbandon = true
      return
    }
    void act(abandonTask)
  }
</script>

{#if visible}
  <div class="banner" role="status">
    <div class="message">任务 {task.name} 已暂停 {fmtHMS(pausedFor)}——继续 / 完成 / 放弃？</div>
    <div class="actions">
      <button disabled={busy} on:click={() => act(resume)}>继续</button>
      <button disabled={busy} on:click={() => act(finishTask)}>完成</button>
      <button disabled={busy} class:confirm={confirmAbandon} on:click={doAbandon}>
        {confirmAbandon ? '确认放弃?' : '放弃'}
      </button>
    </div>
    {#if error}<p class="error">{error}</p>{/if}
  </div>
{/if}

<style>
  .banner { padding: 8px 10px; border-bottom: 1px solid #d69e2e; background: rgba(214, 158, 46, 0.18); }
  .message { font-size: 11px; margin-bottom: 6px; }
  .actions { display: flex; gap: 6px; }
  .actions button { flex: 1; cursor: pointer; font-size: 11px; }
  .actions button.confirm { color: #ffb3ad; border-color: #c0392b; }
  .error { color: #ffb3ad; font-size: 10px; margin: 5px 0 0; }
</style>
