<script lang="ts">
  import { createEventDispatcher, onDestroy } from 'svelte'
  import type { ReminderConfig, Task } from '../lib/api'
  import { pause, resume, finishTask, abandonTask, openSettings, PAUSE_REASONS } from '../lib/api'
  import { pausedChipLayout } from '../lib/pausedChips'
  import { fmtHMS } from '../lib/format'
  import { reminderLevel, reminderPulse, type ReminderPulseState } from '../lib/reminder'
  import SwitchTaskForm from './SwitchTaskForm.svelte'

  export let task: Task
  export let paused: Task[] = []
  export let nowTick = Date.now()
  export let widgetHeight = 0
  export let reminderConfig: ReminderConfig | null = null
  export let pulse: ReminderPulseState = { level: 'none', until: 0, key: '' }
  export let onMuteReminder: (expectedTaskId: string) => Promise<Task> = async () => task

  const dispatch = createEventDispatcher<{ switching: boolean; switched: void; layout: void }>()

  let mode: 'normal' | 'pause-menu' | 'switch-form' = 'normal'
  let confirmAbandon = false
  let error = ''
  let busy = false
  let switchParentId: string | null = null
  let confirmationTaskId = task.id

  $: chips = pausedChipLayout(paused.filter(item => item.id !== task.id), widgetHeight, Boolean(error))

  $: layoutSignature = [mode, chips.rows, chips.hiddenCount, Boolean(error), task.id, task.status].join('|')
  $: if (typeof layoutSignature === 'string') dispatch('layout')
  $: remaining = task.est_seconds - task.net_seconds
  $: pct = task.est_seconds > 0
    ? Math.min(100, Math.round((task.net_seconds / task.est_seconds) * 100)) : 0
  $: overdue = remaining < 0
  $: visualLevel = reminderConfig
    ? reminderLevel(task.net_seconds, task.est_seconds, reminderConfig)
    : overdue ? 'overdue' : 'none'
  $: pulseStrength = pulse.until > nowTick ? reminderPulse(pulse.level) : 'none'
  $: dueState = visualLevel === 'due' || visualLevel === 'overdue'

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

  function doResume() {
    const expectedTaskId = task.id
    void act(() => resume(expectedTaskId))
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

  function doMuteReminder() {
    if (busy || task.reminder_muted) return
    const expectedTaskId = task.id
    void act(async () => {
      const updated = await onMuteReminder(expectedTaskId)
      if (task.id !== expectedTaskId || updated.id !== expectedTaskId) {
        throw new Error('task changed, please refresh')
      }
      task = updated
    })
  }

  onDestroy(() => dispatch('switching', false))
</script>

<div class="widget" class:due-state={dueState}>
  {#if pulseStrength !== 'none'}
    {#key pulse.key}
      <span
        class="pulse-overlay"
        class:pulse-gentle={pulseStrength === 'gentle'}
        class:pulse-strong={pulseStrength === 'strong'}
        aria-hidden="true"
      ></span>
    {/key}
  {/if}
  <div class="head">
    <span class="name">{task.name}</span>
    <span class="head-actions">
      <span class="tag">{task.tag ?? ''}</span>
      <button class="settings-button" type="button" title="提醒设置" aria-label="提醒设置" on:click={openSettings}>⚙</button>
    </span>
  </div>
  <div
    class="time"
    class:warn={visualLevel === 'warn'}
    class:near={visualLevel === 'near'}
    class:due={visualLevel === 'due'}
    class:overdue={visualLevel === 'overdue'}
  >{fmtHMS(task.net_seconds)}</div>

  {#if mode === 'switch-form' && switchParentId !== null}
    <SwitchTaskForm
      parentTaskId={switchParentId}
      on:success={completeSwitch}
      on:cancel={cancelSwitch}
    />
  {:else if mode === 'pause-menu'}
    <div class="pause-menu">
      {#each PAUSE_REASONS as reason}
        <button disabled={busy} on:click={() => doPause(reason.value)}>{reason.label}</button>
      {/each}
      <button disabled={busy} class="cancel" on:click={() => (mode = 'normal')}>取消</button>
    </div>
  {:else if task.status === 'running'}
    <div class="foot">
      {#if overdue}
        <span class="rem">超时 +{fmtHMS(-remaining)} · {pct}%</span>
      {:else}
        <span class="rem">剩余 {fmtHMS(remaining)} · {pct}%</span>
      {/if}
      <span class="actions">
        {#if overdue}
          <button class="mute" disabled={busy || task.reminder_muted} on:click={doMuteReminder}>
            [{task.reminder_muted ? '已静音' : '静音'}]
          </button>
        {/if}
        <button disabled={busy} title="暂停" on:click={() => (mode = 'pause-menu')}>⏸</button>
        <button disabled={busy} title="完成" on:click={doFinish}>✔</button>
        <button disabled={busy} title="放弃" on:click={doAbandon}>{confirmAbandon ? '确认放弃?' : '✕'}</button>
      </span>
    </div>
  {:else}
    <div class="foot">
      <span class="rem paused">已暂停</span>
      <span class="actions">
        <button disabled={busy} title="继续" on:click={doResume}>▶</button>
        <button disabled={busy} title="完成" on:click={doFinish}>✔</button>
        <button disabled={busy} title="放弃" on:click={doAbandon}>{confirmAbandon ? '确认放弃?' : '✕'}</button>
      </span>
    </div>
    {#if chips.rows > 0}
      <div class="chips" aria-label="其他暂停任务">
        {#each chips.visible as item (item.id)}
          <span class="chip" title={`${item.name} · 已暂停 ${fmtHMS(pausedFor(item))}`}>{item.name} · 已暂停 {fmtHMS(pausedFor(item))}</span>
        {/each}
        {#if chips.hiddenCount > 0}
          <span class="chip collapsed" title={chips.allNames} aria-label={`另有 ${chips.hiddenCount} 个暂停任务；全部任务：${chips.allNames}`}>+{chips.hiddenCount}</span>
        {/if}
      </div>
    {/if}
  {/if}

  {#if error}<p class="error" title={error}>{error}</p>{/if}
</div>

<style>
  .widget { position: relative; isolation: isolate; flex: none; min-width: 0; min-height: 126px; padding: 3px 6px; overflow: hidden; }
  .head { display: flex; height: 14px; line-height: 14px; justify-content: space-between; align-items: baseline; gap: 4px; overflow: hidden; }
  .name { min-width: 0; font-size: 11px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .head-actions { display: flex; flex: none; height: 14px; align-items: center; gap: 2px; overflow: hidden; }
  .tag { flex: none; max-width: 48px; overflow: hidden; text-overflow: ellipsis; font-size: 10px; opacity: 0.6; }
  .head .settings-button { flex: none; width: 15px; height: 14px; min-height: 0; padding: 0; border: 0; line-height: 14px; background: none; color: #c8c9cc; cursor: pointer; font-size: 10px; }
  .time { height: 28px; line-height: 28px; font-size: 26px; font-variant-numeric: tabular-nums; text-align: center; }
  .time.warn { color: #f1c40f; }
  .time.near { color: #e67e22; }
  .time.due, .time.overdue, .widget.due-state .rem { color: #e74c3c; }
  .widget.due-state { box-shadow: inset 0 0 0 2px #c0392b; }
  .pulse-overlay { position: absolute; z-index: -1; inset: 0; pointer-events: none; }
  .pulse-overlay.pulse-gentle { animation: pulse-gentle 3s ease-out; }
  .pulse-overlay.pulse-strong { animation: pulse-strong 3s ease-out; }
  .foot { display: flex; height: 20px; justify-content: space-between; align-items: center; gap: 2px; font-size: 10px; }
  .rem { min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .paused { opacity: 0.6; }
  .actions { display: flex; flex: none; }
  .actions button { height: 20px; border: none; background: none; cursor: pointer; font-size: 12px; padding: 1px 3px; }
  .actions button.mute { font-size: 10px; padding-inline: 1px; }
  .pause-menu { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); grid-auto-rows: 20px; height: 44px; margin-top: 6px; gap: 4px; }
  .pause-menu button { width: 100%; min-width: 0; height: 20px; font-size: 10px; padding: 1px 2px; cursor: pointer; }
  .pause-menu button.cancel { opacity: 0.6; }
  .actions button:disabled, .pause-menu button:disabled { cursor: default; opacity: 0.45; }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; max-height: 48px; margin-top: 4px; overflow: hidden; }
  .chip { flex: 0 0 calc(50% - 2px); min-width: 0; height: 18px; padding: 2px 4px; line-height: 14px; border-radius: 999px; background: rgba(255, 255, 255, 0.1); font-size: 10px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .collapsed { text-align: center; font-weight: 600; }
  .error { height: 12px; line-height: 12px; margin: 2px 0 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: #c0392b; font-size: 10px; }

  @keyframes pulse-gentle {
    0%, 35% { background: rgba(241, 196, 15, 0.18); }
    100% { background: transparent; }
  }
  @keyframes pulse-strong {
    0%, 18%, 36% { background: rgba(231, 76, 60, 0.28); box-shadow: inset 0 0 0 2px #e74c3c; }
    9%, 27%, 100% { background: transparent; box-shadow: inset 0 0 0 2px rgba(192, 57, 43, 0.35); }
  }
</style>
