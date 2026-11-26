<script lang="ts">
  import { createEventDispatcher } from 'svelte'
  import { createTask, TAG_PRESETS } from '../lib/api'

  export let parentTaskId: string

  const dispatch = createEventDispatcher<{ cancel: void; success: void }>()

  let name = ''
  let estMin = 25
  let tag = localStorage.getItem('traceTime:lastTag') ?? '科研'
  let busy = false
  let error = ''

  async function submit() {
    if (busy || !name.trim()) return
    if (!Number.isInteger(estMin) || estMin < 1) {
      error = '请输入至少 1 分钟的整数时长'
      return
    }

    busy = true
    error = ''
    try {
      await createTask(
        name,
        estMin * 60,
        tag === '__none__' ? undefined : tag,
        parentTaskId,
      )
      if (tag !== '__none__') localStorage.setItem('traceTime:lastTag', tag)
      dispatch('success')
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }
</script>

<form class="switch-form" on:submit|preventDefault={submit}>
  <input class="name" placeholder="新任务名" bind:value={name} required />
  <div class="row">
    <input
      class="duration"
      type="number"
      min="1"
      step="1"
      bind:value={estMin}
      title="预计分钟"
      aria-label="预计分钟"
    />
    <select bind:value={tag} aria-label="标签">
      {#each TAG_PRESETS as t}<option value={t}>{t}</option>{/each}
      <option value="__none__">无标签</option>
    </select>
  </div>
  <div class="row actions">
    <button type="submit" disabled={busy || !name.trim()}>开始新任务</button>
    <button type="button" class="ghost" on:click={() => dispatch('cancel')}>取消</button>
  </div>
  {#if error}<p class="error">{error}</p>{/if}
</form>

<style>
  .switch-form { display: flex; flex-direction: column; gap: 6px; padding-top: 8px; }
  .name { width: 100%; }
  .row { display: flex; gap: 6px; }
  .row > * { flex: 1; min-width: 0; }
  .duration { width: 70px; }
  .actions button { cursor: pointer; }
  .ghost { opacity: 0.65; }
  .error { color: #c0392b; font-size: 10px; margin: 0; }
</style>
