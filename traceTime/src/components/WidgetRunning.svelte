<script lang="ts">
  import { createEventDispatcher, onDestroy } from 'svelte'
  import type { Task } from '../lib/api'
  import { pause, resume, finishTask, abandonTask, PAUSE_REASONS } from '../lib/api'
  import { fmtHMS } from '../lib/format'
  import SwitchTaskForm from './SwitchTaskForm.svelte'

  export let task: Task
  export let paused: Task[] = []
  export let nowTick = Date.now()

  const dispatch = createEventDispatcher<{ switching: boolean; switched: void }>()

  let mode: 'normal' | 'pause-menu' | 'switch-form' = 'normal'
  let confirmAbandon = false
  let error = ''
  let busy = false
  let switchParentId: string | null = null
  let confirmationTaskId = task.id

  $: remaining = task.est_seconds - task.net_seconds
  $: pct = Math.min(100, Math.round((task.net_seconds / task.est_seconds) * 100))
  $: overdue = remaining < 0

  $: if (mode === 'switch-form' &&
    (task.id !== switchParentId || task.status !== 'paused')) {
    cancelSwitch() // Unmounting SwitchTaskForm discards its local draft.
  }
  $: if (task.id !== confirmationTaskId) {
    confirmationTaskId = task.id
    confirmAbandon = false
  }

  function pausedFor(item: Task) {
    return item.paused_since
      ? Math.max(0, (nowTick - Date.parse(item.paused_since)) / 1000) : 0
  }

  async function act(action: () => Promise<unknown>) {
    if (busy) return
    busy = true
    error = ''
    try { await action() } catch (e) { error = String(e) }
    finally { busy = false }
  }

  async function doPause(reason: string) {
    const parentId = task.id
    await act(async () => {
      const pausedTask = await pause(reason)
      if (task.id !== parentId || pausedTask.id !== parentId) {
        throw new Error('task changed, please refresh')
      }
      // Use the pause response now; the next poll may not have arrived yet.
      task = pausedTask
      switchParentId = reason === 'switch' ? parentId : null
      mode = reason === 'switch' ? 'switch-form' : 'normal'
      if (reason === 'switch') dispatch('switching', true)
    })
  }

  function cancelSwitch() {
    mode = 'normal'
    switchParentId = null
    dispatch('switching', false)
  }

  function completeSwitch() {
    cancelSwitch()
    dispatch('switched')
  }

  function doFinish() {
    const expectedTaskId = task.id
    void act(() => finishTask(expectedTaskId))
  }

  function doAbandon() {
    if (busy) return
    if (!confirmAbandon) {
      confirmAbandon = true
      return
    }
    const expectedTaskId = task.id
    void act(() => abandonTask(expectedTaskId))
  }

  onDestroy(() => dispatch('switching', false))
</script>

<div class="widget" class:overdue>
  <div class="head">
    <span class="name">{task.name}</span>
    <span class="tag">{task.tag ?? ''}</span>
  </div>
  <div class="time" class:overdue>{fmtHMS(task.net_seconds)}</div>

  {#if mode === 'switch-form' && switchParentId !== null}
    <SwitchTaskForm
      parentTaskId={switchParentId}
      on:success={completeSwitch}
      on:cancel={cancelSwitch}
    />
  {:else if task.status === 'running'}
    <div class="foot">
      {#if overdue}
        <span class="rem">超时 +{fmtHMS(-remaining)} · {pct}%</span>
      {:else}
        <span class="rem">剩余 {fmtHMS(remaining)} · {pct}%</span>
      {/if}
      <span class="actions">
        <button disabled={busy} title="暂停" on:click={() => (mode = 'pause-menu')}>⏸</button>
        <button disabled={busy} title="完成" on:click={doFinish}>✔</button>
        <button disabled={busy} title="放弃" on:click={doAbandon}>{confirmAbandon ? '确认放弃?' : '✕'}</button>
      </span>
    </div>
  {:else}
    <div class="foot">
      <span class="rem paused">已暂停</span>
      <span class="actions">
        <button disabled={busy} title="继续" on:click={() => act(resume)}>▶</button>
        <button disabled={busy} title="完成" on:click={doFinish}>✔</button>
        <button disabled={busy} title="放弃" on:click={doAbandon}>{confirmAbandon ? '确认放弃?' : '✕'}</button>
      </span>
    </div>
    {#if paused.length > 1}
      <div class="chips" aria-label="其他暂停任务">
        {#each paused.filter(item => item.id !== task.id) as item (item.id)}
          <span class="chip" title={item.name}>{item.name} · 已暂停 {fmtHMS(pausedFor(item))}</span>
        {/each}
      </div>
    {/if}
  {/if}

  {#if mode === 'pause-menu'}
    <div class="pause-menu">
      {#each PAUSE_REASONS as reason}
        <button disabled={busy} on:click={() => doPause(reason.value)}>{reason.label}</button>
      {/each}
      <button disabled={busy} class="cancel" on:click={() => (mode = 'normal')}>取消</button>
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
  .pause-menu button.cancel { opacity: 0.6; }
  .actions button:disabled, .pause-menu button:disabled { cursor: default; opacity: 0.45; }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 6px; }
  .chip { max-width: 100%; padding: 3px 6px; border-radius: 999px; background: rgba(255, 255, 255, 0.1); font-size: 10px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .error { color: #c0392b; font-size: 10px; }
</style>
