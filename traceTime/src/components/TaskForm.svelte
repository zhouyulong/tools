<script lang="ts">
  import { onMount } from 'svelte'
  import { createTask, EST_PRESETS, openHistory, TAG_PRESETS } from '../lib/api'

  export let refreshAll: () => Promise<void> = async () => {}

  let name = ''
  let duration = '25'
  let customMin: number | undefined = 25
  let tag = localStorage.getItem('traceTime:lastTag') ?? '__none__'
  let error = ''
  let busy = false

  onMount(() => {
    void refreshAll()
  })

  async function submit() {
    if (busy) return
    const estMin = duration === 'custom' ? customMin : Number(duration)
    if (estMin === undefined || !Number.isInteger(estMin) || estMin < 1) {
      error = '请输入至少 1 分钟的整数时长'
      return
    }
    busy = true
    error = ''
    try {
      await createTask(name, estMin * 60, tag === '__none__' ? undefined : tag)
      if (tag !== '__none__') localStorage.setItem('traceTime:lastTag', tag)
      await refreshAll()
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }
</script>

<div class="task-form">
  <div class="form-header">
    <span>开始任务</span>
    <button class="ghost" type="button" on:click={openHistory}>历史</button>
  </div>

  <form class="form" on:submit|preventDefault={submit}>
    <input class="name" placeholder="任务名" bind:value={name} required />
    <div class="row">
      <select aria-label="预估时长" bind:value={duration}>
        {#each EST_PRESETS as m}
          <option value={String(m)}>{m} 分钟</option>
        {/each}
        <option value="custom">自定义</option>
      </select>
      <select aria-label="标签" bind:value={tag}>
        {#each TAG_PRESETS as t}
          <option value={t}>{t}</option>
        {/each}
        <option value="__none__">无标签</option>
      </select>
      <button type="submit" disabled={busy || !name.trim()}>开始</button>
    </div>
    {#if duration === 'custom'}
      <label class="custom-duration">
        自定义分钟
        <input type="number" min="1" step="1" required bind:value={customMin} />
      </label>
    {/if}
    {#if error}<p class="error">{error}</p>{/if}
  </form>
</div>

<style>
  .task-form { padding: 10px; }
  .form-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 6px; color: #aaaeb5; font-size: 11px; }
  .ghost { padding: 2px 7px; border: 1px solid rgba(255, 255, 255, 0.16); border-radius: 4px; color: #d8d8da; background: transparent; cursor: pointer; }
  .ghost:hover { background: rgba(255, 255, 255, 0.08); }
  .form { display: flex; flex-direction: column; gap: 6px; }
  .row { display: flex; gap: 6px; }
  .name { width: 100%; box-sizing: border-box; }
  .custom-duration { display: flex; align-items: center; gap: 6px; font-size: 12px; }
  .custom-duration input { width: 70px; }
  .error { color: #c0392b; font-size: 11px; margin: 0; }
</style>
