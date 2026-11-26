<script lang="ts">
  import { onMount } from 'svelte'
  import { listFinished, taskDetail, deleteTask, deleteTasks, type Task, type TaskDetail } from '../lib/api'
  import { exportTaskDetail } from '../lib/taskExport'
  import { historyTimeline, wallClockSeconds, segmentSeconds, durationPercent } from '../lib/history'
  import { fmtHMS } from '../lib/format'
  import TimelineBar from './TimelineBar.svelte'

  let tasks: Task[] = []
  let selected: TaskDetail | null = null
  let groups: { day: string; items: Task[] }[] = []
  let loading = true
  let detailLoading = false
  let error = ''
  let selectSeq = 0
  let listSeq = 0
  let selectedId: string | null = null
  let confirmDeleteId: string | null = null
  let deleting = false
  let selectedIds: string[] = []
  let confirmBatchDelete = false
  let exporting = false
  const deletedIds = new Set<string>()

  $: timeline = historyTimeline(selected)
  $: wallSeconds = selected ? wallClockSeconds(selected.task, timeline.segments) : 0
  $: allSelected = tasks.length > 0 && selectedIds.length === tasks.length

  function dayOf(task: Task): string {
    return new Date(task.finished_at!).toLocaleDateString('zh-CN')
  }

  function regroup() {
    const map = new Map<string, Task[]>()
    for (const task of tasks) {
      const day = dayOf(task)
      if (!map.has(day)) map.set(day, [])
      map.get(day)!.push(task)
    }
    groups = [...map.entries()].map(([day, items]) => ({ day, items }))
  }

  function deviation(task: Task): number {
    return task.net_seconds - task.est_seconds
  }

  function deviationText(task: Task): string {
    const delta = deviation(task)
    if (delta > 0) return `超 ${fmtHMS(delta)}`
    if (delta < 0) return `少 ${fmtHMS(-delta)}`
    return '准时'
  }

  function localTime(value: string): string {
    return new Date(value).toLocaleString('zh-CN', {
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    })
  }

  function kindLabel(kind: string): string {
    if (kind === 'work') return '工作'
    if (kind === 'pause') return '暂停'
    return kind
  }

  function toggleAll(event: Event) {
    const checked = (event.currentTarget as HTMLInputElement).checked
    selectedIds = checked ? tasks.map(task => task.id) : []
    confirmBatchDelete = false
  }

  async function select(task: Task) {
    if (deleting) return
    confirmDeleteId = null
    confirmBatchDelete = false
    selected = null
    selectedId = task.id
    selectedIds = selectedIds.filter(id => id !== task.id)
    const seq = ++selectSeq
    detailLoading = true
    error = ''
    try {
      const detail = await taskDetail(task.id)
      if (seq !== selectSeq) return
      selected = detail
    } catch (e) {
      if (seq !== selectSeq) return
      error = String(e)
    } finally {
      if (seq === selectSeq) detailLoading = false
    }
  }

  async function reload() {
    const seq = ++listSeq
    error = ''
    try {
      const result = await listFinished()
      if (seq !== listSeq) return
      tasks = result.filter(task => !deletedIds.has(task.id))
      selectedIds = selectedIds.filter(id => tasks.some(task => task.id === id))
      if (selectedIds.length === 0) confirmBatchDelete = false
      regroup()
      if (selectedId && !tasks.some(task => task.id === selectedId)) {
        selectedId = null
        selected = null
        confirmDeleteId = null
        selectedIds = selectedIds.filter(id => id !== selectedId)
        ++selectSeq // Invalidate an outstanding detail request as well.
        detailLoading = false
      }
    } catch (e) {
      if (seq === listSeq) error = String(e)
    } finally {
      if (seq === listSeq) loading = false
    }
  }

  async function removeSelected() {
    if (!selected || deleting || detailLoading) return
    const id = selected.task.id
    if (confirmDeleteId !== id) {
      confirmDeleteId = id
      return
    }
    deleting = true
    error = ''
    try {
      await deleteTask(id)
      deletedIds.add(id) // A focus refresh already in flight cannot resurrect this row.
      tasks = tasks.filter(task => task.id !== id)
      regroup()
      ++selectSeq
      selectedId = null
      selected = null
      detailLoading = false
      confirmDeleteId = null
    } catch (e) {
      error = String(e)
    } finally {
      deleting = false
    }
  }

  async function removeMany() {
    if (deleting || selectedIds.length === 0) return
    if (!confirmBatchDelete) {
      confirmBatchDelete = true
      return
    }
    deleting = true
    error = ''
    const ids = [...selectedIds]
    try {
      await deleteTasks(ids)
      for (const id of ids) deletedIds.add(id)
      selectedIds = []
      confirmBatchDelete = false
      tasks = tasks.filter(task => !ids.includes(task.id))
      regroup()
      if (selectedId && ids.includes(selectedId)) {
        ++selectSeq
        selectedId = null
        selected = null
        detailLoading = false
        confirmDeleteId = null
      }
    } catch (e) {
      error = String(e)
    } finally {
      deleting = false
    }
  }

  async function exportSelected() {
    if (!selected || exporting || detailLoading) return
    exporting = true
    error = ''
    try {
      const saved = await exportTaskDetail(selected)
      if (!saved) error = '已取消导出'
    } catch (e) {
      error = String(e)
    } finally {
      exporting = false
    }
  }

  onMount(() => {
    const onFocus = () => { void reload() }
    window.addEventListener('focus', onFocus)
    void reload()
    return () => {
      window.removeEventListener('focus', onFocus)
      ++listSeq
      ++selectSeq
    }
  })
</script>

<div class="history">
  <aside>
    <div class="list-header">
      <h2>历史任务</h2>
      <label class="batch-select"><input type="checkbox" checked={allSelected} disabled={deleting || tasks.length === 0} on:change={toggleAll} />全选</label>
    </div>
    {#if selectedIds.length > 0}
      <div class="batch-actions">
        <button disabled={deleting} on:click={removeMany}>
          {deleting ? '正在删除…' : confirmBatchDelete ? `确认删除所选（${selectedIds.length}）` : `删除所选（${selectedIds.length}）`}
        </button>
        {#if confirmBatchDelete}<button disabled={deleting} on:click={() => (confirmBatchDelete = false)}>取消</button>{/if}
      </div>
    {/if}
    {#if loading}
      <p class="empty">正在加载…</p>
    {:else}
      {#each groups as group}
        <p class="day">{group.day}</p>
        {#each group.items as task}
          <button disabled={deleting} class:on={selected?.task.id === task.id} on:click={() => select(task)}>
            <span class="task-line">
              <input
                type="checkbox"
                checked={selectedIds.includes(task.id)}
                value={task.id}
                bind:group={selectedIds}
                disabled={deleting}
                on:click|stopPropagation
                aria-label={`选择 ${task.name}`}
              />
              <strong>{task.name}</strong>
              {#if task.status === 'abandoned'}<span class="badge">已放弃</span>{/if}
            </span>
            <span class="task-meta">
              <span>净 {fmtHMS(task.net_seconds)} · 预计 {fmtHMS(task.est_seconds)}</span>
              <em class:over={deviation(task) > 0}>{deviationText(task)}</em>
            </span>
          </button>
        {/each}
      {:else}
        <p class="empty">暂无历史任务</p>
      {/each}
    {/if}
  </aside>

  <section>
    {#if error}<p class="error">{error}</p>{/if}
    {#if detailLoading}
      <p class="empty">正在加载详情…</p>
    {:else if selected}
      <div class="heading">
        <h3>{selected.task.name}</h3>
        {#if selected.task.status === 'abandoned'}<span class="badge">已放弃</span>{/if}
      </div>
      <div class="delete-actions">
        <button disabled={deleting} on:click={exportSelected}>{exporting ? '正在导出…' : '导出'}</button>
        <button disabled={deleting} on:click={removeSelected}>{deleting ? '正在删除…' : confirmDeleteId === selected.task.id ? '确认删除' : '删除'}</button>
        {#if confirmDeleteId === selected.task.id}
          <span>仅从历史列表隐藏，轨迹保留。</span>
          <button disabled={deleting} on:click={() => (confirmDeleteId = null)}>取消</button>
        {/if}
      </div>
      <div class="stats">
        <span><small>净时长</small>{fmtHMS(selected.task.net_seconds)}</span>
        <span><small>墙钟</small>{fmtHMS(wallSeconds)}</span>
        <span><small>预计</small>{fmtHMS(selected.task.est_seconds)}</span>
        <span class:over={deviation(selected.task) > 0}>
          <small>偏差</small>{deviationText(selected.task)}
        </span>
      </div>
      <p class="pause-stats">暂停 {timeline.pauseCount} 次 / 共 {fmtHMS(timeline.pauseSeconds)}（占 {durationPercent(timeline.pauseSeconds, wallSeconds)}%）</p>
      <TimelineBar segments={timeline.segments} {wallSeconds} />
      <div class="segments">
        {#each timeline.segments as segment}
          <p class="seg">
            <span class="kind {segment.kind === 'work' ? 'work' : segment.kind === 'pause' ? 'pause' : 'other'}">
              {kindLabel(segment.kind)}
            </span>
            <span>{localTime(segment.started_at)} → {localTime(segment.ended_at)}<br />{fmtHMS(segmentSeconds(segment))}（占 {durationPercent(segmentSeconds(segment), wallSeconds)}%）</span>
          </p>
        {:else}
          <p class="empty">暂无时间段记录</p>
        {/each}
      </div>
    {:else}
      <p class="empty centered">选择左侧任务查看轨迹</p>
    {/if}
  </section>
</div>

<style>
  .history {
    display: grid;
    grid-template-columns: 190px minmax(0, 1fr);
    height: 100%;
    min-height: 0;
    overflow: hidden;
    color: #f0f0f0;
    background: #1e1e22;
  }
  aside {
    min-width: 0;
    min-height: 0;
    padding: 14px 10px;
    overflow-y: auto;
    border-right: 1px solid rgba(255, 255, 255, 0.1);
    background: #242429;
  }
  .list-header { display: flex; align-items: center; justify-content: space-between; gap: 6px; margin: 0 6px 8px; }
  h2 { margin: 0; font-size: 16px; }
  .batch-select { display: inline-flex; align-items: center; gap: 3px; color: #aeb2b8; font-size: 10px; cursor: pointer; }
  .batch-actions { display: flex; flex-wrap: wrap; gap: 4px; margin: 0 6px 10px; }
  .batch-actions button { cursor: pointer; font-size: 10px; }
  .day { margin: 12px 6px 5px; color: #9398a0; font-size: 10px; font-weight: 700; }
  aside button {
    display: flex;
    width: 100%;
    flex-direction: column;
    gap: 4px;
    padding: 8px;
    border: 0;
    border-radius: 6px;
    color: inherit;
    background: transparent;
    text-align: left;
    cursor: pointer;
  }
  aside button:hover { background: rgba(255, 255, 255, 0.06); }
  aside button.on { background: rgba(75, 142, 100, 0.25); }
  .task-line, .heading { display: flex; min-width: 0; align-items: center; gap: 6px; }
  .task-line strong { overflow: hidden; font-size: 12px; text-overflow: ellipsis; white-space: nowrap; }
  .task-meta { display: flex; flex-wrap: wrap; gap: 3px; width: 100%; justify-content: space-between; color: #aeb2b8; font-size: 10px; }
  em { color: #7fc89a; font-style: normal; }
  .over { color: #ef7b70 !important; }
  .badge { flex: none; padding: 2px 5px; border-radius: 999px; color: #f0aaa4; background: rgba(207, 87, 77, 0.2); font-size: 9px; }
  section { min-width: 0; min-height: 0; padding: 18px; overflow-y: auto; }
  h3 { margin: 0; font-size: 18px; }
  .delete-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; margin-top: 10px; color: #aeb2b8; font-size: 10px; }
  .delete-actions button { cursor: pointer; }
  .stats { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; margin-top: 14px; }
  .stats span { padding: 8px; border-radius: 6px; background: rgba(255, 255, 255, 0.055); font-size: 12px; }
  .stats small { display: block; margin-bottom: 3px; color: #969ba3; font-size: 9px; }
  .pause-stats { color: #aeb2b8; font-size: 11px; }
  .segments { margin-top: 14px; }
  .seg { display: flex; align-items: center; gap: 8px; margin: 0; padding: 7px 0; border-bottom: 1px solid rgba(255, 255, 255, 0.07); color: #b8bcc2; font-size: 10px; }
  .kind { min-width: 34px; padding: 2px 4px; border-radius: 3px; color: #fff; text-align: center; }
  .kind.work { background: #39a96b; }
  .kind.pause { background: #7f8790; }
  .kind.other { background: #cf574d; }
  .empty { color: #90959d; font-size: 12px; }
  .centered { margin-top: 40%; text-align: center; }
  .error { padding: 8px; border-radius: 5px; color: #ffb2ab; background: rgba(207, 87, 77, 0.15); font-size: 11px; }
</style>
