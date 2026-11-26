<script lang="ts">
  import { createTask, EST_PRESETS, TAG_PRESETS } from '../lib/api'

  let name = ''
  let estMin = 25
  let tag = localStorage.getItem('traceTime:lastTag') ?? '科研'
  let error = ''
  let busy = false

  async function submit() {
    if (busy) return
    busy = true; error = ''
    try {
      await createTask(name, estMin * 60, tag)
      localStorage.setItem('traceTime:lastTag', tag)
    } catch (e) { error = String(e) }
    busy = false
  }
</script>

<form class="form" on:submit|preventDefault={submit}>
  <input class="name" placeholder="任务名" bind:value={name} required />
  <div class="row">
    <select bind:value={estMin}>
      {#each EST_PRESETS as m}
        <option value={m}>{m} 分钟</option>
      {/each}
    </select>
    <select bind:value={tag}>
      {#each TAG_PRESETS as t}
        <option value={t}>{t}</option>
      {/each}
    </select>
    <button type="submit" disabled={busy || !name.trim()}>开始</button>
  </div>
  {#if error}<p class="error">{error}</p>{/if}
</form>

<style>
  .form { display: flex; flex-direction: column; gap: 6px; padding: 10px; }
  .row { display: flex; gap: 6px; }
  .name { width: 100%; }
  .error { color: #c0392b; font-size: 11px; margin: 0; }
</style>
