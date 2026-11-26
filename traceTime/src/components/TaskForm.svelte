<script lang="ts">
  import { onMount } from 'svelte'
  import { createTask, EST_PRESETS, resume, TAG_PRESETS, type Task } from '../lib/api'
  import { fmtHMS } from '../lib/format'

  export let paused: Task[] = []
  export let nowTick = Date.now()
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

  function pausedFor(task: Task) {
    if (!task.paused_since) return 0
    return Math.max(0, (nowTick - new Date(task.paused_since).getTime()) / 1000)
  }

  async function resumeLatest() {
    if (busy) return
    busy = true
    error = ''
    try {
      await resume()
      await refreshAll()
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }

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
  {#if paused.length}
    <div class="paused-list">
      <div class="chips">
        {#each paused as task}
          <span class="chip" title={task.name}>{task.name} · 已暂停 {fmtHMS(pausedFor(task))}</span>
        {/each}
      </div>
      <button class="resume" disabled={busy} on:click={resumeLatest}>继续最近暂停的任务</button>
    </div>
  {/if}

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
  .form { display: flex; flex-direction: column; gap: 6px; }
  .row { display: flex; gap: 6px; }
  .name { width: 100%; box-sizing: border-box; }
  .custom-duration { display: flex; align-items: center; gap: 6px; font-size: 12px; }
  .custom-duration input { width: 70px; }
  .paused-list { display: flex; flex-direction: column; gap: 6px; padding-bottom: 10px; margin-bottom: 10px; border-bottom: 1px solid rgba(255, 255, 255, 0.12); }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; }
  .chip { max-width: 100%; padding: 3px 6px; border-radius: 999px; background: rgba(255, 255, 255, 0.1); font-size: 10px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .resume { align-self: flex-start; cursor: pointer; font-size: 11px; }
  .error { color: #c0392b; font-size: 11px; margin: 0; }
</style>
