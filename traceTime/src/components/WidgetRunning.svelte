<script lang="ts">
  import type { Task } from '../lib/api'
  import { pause, resume, finishTask, abandonTask, PAUSE_REASONS } from '../lib/api'
  import { fmtHMS } from '../lib/format'

  export let task: Task
  let showPauseMenu = false
  let confirmAbandon = false
  let error = ''

  $: remaining = task.est_seconds - task.net_seconds
  $: pct = Math.min(100, Math.round((task.net_seconds / task.est_seconds) * 100))
  $: overdue = remaining < 0

  async function doPause(reason: string) {
    showPauseMenu = false
    try { await pause(reason) } catch (e) { error = String(e) }
  }
  async function doAbandon() {
    if (!confirmAbandon) { confirmAbandon = true; return }
    try { await abandonTask() } catch (e) { error = String(e) }
  }
</script>

<div class="widget" class:overdue>
  <div class="head">
    <span class="name">{task.name}</span>
    <span class="tag">{task.tag ?? ''}</span>
  </div>
  <div class="time" class:overdue>{fmtHMS(task.net_seconds)}</div>
  {#if task.status === 'running'}
    <div class="foot">
      {#if overdue}
        <span class="rem">超时 +{fmtHMS(-remaining)} · {pct}%</span>
      {:else}
        <span class="rem">剩余 {fmtHMS(remaining)} · {pct}%</span>
      {/if}
      <span class="actions">
        <button title="暂停" on:click={() => (showPauseMenu = true)}>⏸</button>
        <button title="完成" on:click={() => finishTask().catch(e => error = String(e))}>✔</button>
        <button title="放弃" on:click={doAbandon}>{confirmAbandon ? '确认放弃?' : '✕'}</button>
      </span>
    </div>
  {:else}
    <div class="foot">
      <span class="rem paused">已暂停</span>
      <span class="actions">
        <button title="继续" on:click={() => resume().catch(e => error = String(e))}>▶</button>
        <button title="完成" on:click={() => finishTask().catch(e => error = String(e))}>✔</button>
        <button title="放弃" on:click={doAbandon}>{confirmAbandon ? '确认放弃?' : '✕'}</button>
      </span>
    </div>
  {/if}
  {#if showPauseMenu}
    <div class="pause-menu">
      {#each PAUSE_REASONS as r}
        <button on:click={() => doPause(r.value)}>{r.label}</button>
      {/each}
    </div>
  {/if}
  {#if error}<p class="error">{error}</p>{/if}
</div>

<style>
  .widget { padding: 10px; position: relative; }
  .head { display: flex; justify-content: space-between; align-items: baseline; }
  .name { font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 150px; }
  .tag { font-size: 10px; opacity: 0.6; }
  .time { font-size: 34px; font-variant-numeric: tabular-nums; text-align: center; }
  .time.overdue, .widget.overdue .rem { color: #c0392b; }
  .widget.overdue { outline: 2px solid #c0392b; }
  .foot { display: flex; justify-content: space-between; align-items: center; font-size: 12px; }
  .paused { opacity: 0.6; }
  .actions button { border: none; background: none; cursor: pointer; font-size: 13px; padding: 2px 4px; }
  .pause-menu { position: absolute; inset: auto 8px 8px 8px; display: flex; gap: 4px; }
  .pause-menu button { flex: 1; font-size: 10px; padding: 4px 2px; cursor: pointer; }
  .error { color: #c0392b; font-size: 10px; }
</style>
