<script lang="ts">
  import type { Segment } from '../lib/api'
  import { segmentSeconds, durationPercent } from '../lib/history'
  import { fmtHMS } from '../lib/format'

  export let segments: Segment[] = []
  export let wallSeconds = 0

  $: positioned = segments
    .map((segment) => ({
      segment,
      start: new Date(segment.started_at).getTime(),
      end: new Date(segment.ended_at).getTime(),
    }))
    .filter(({ segment, start, end }) =>
      Number.isFinite(start) && Number.isFinite(end) && end >= start &&
      // A clamped rollback pause must not become a minimum-width grey work overlap.
      (segment.kind !== 'pause' || end > start))
  $: first = positioned.length ? Math.min(...positioned.map(({ start }) => start)) : 0
  $: last = positioned.length ? Math.max(...positioned.map(({ end }) => end)) : 0
  $: span = Math.max(1, last - first)

  function kindClass(kind: string) {
    if (kind === 'work') return 'work'
    if (kind === 'pause') return 'pause'
    return 'other'
  }
</script>

{#if positioned.length}
  <div class="timeline" aria-label="任务时间轴">
    {#each positioned as item}
      <span
        class="segment {kindClass(item.segment.kind)}"
        style:left={`${((item.start - first) / span) * 100}%`}
        style:width={`${Math.max(0.8, ((item.end - item.start) / span) * 100)}%`}
        title={`${item.segment.kind}: ${item.segment.started_at} → ${item.segment.ended_at} · ${fmtHMS(segmentSeconds(item.segment))}（占 ${durationPercent(segmentSeconds(item.segment), wallSeconds)}%）`}
      >{fmtHMS(segmentSeconds(item.segment))}（{durationPercent(segmentSeconds(item.segment), wallSeconds)}%）</span>
    {/each}
  </div>
  <div class="legend" aria-hidden="true">
    <span><i class="work"></i>工作</span>
    <span><i class="pause"></i>暂停</span>
    <span><i class="other"></i>其他</span>
  </div>
{:else}
  <p class="empty">暂无时间段</p>
{/if}

<style>
  .timeline {
    position: relative;
    height: 18px;
    margin: 12px 0 6px;
    overflow: hidden;
    border-radius: 5px;
    background: rgba(255, 255, 255, 0.08);
  }
  .segment { position: absolute; top: 0; bottom: 0; min-width: 2px; overflow: hidden; white-space: nowrap; color: #fff; font-size: 9px; line-height: 18px; }
  .work { background: #39a96b; }
  .pause { background: #7f8790; }
  .other { background: #cf574d; }
  .legend { display: flex; gap: 12px; color: #aeb2b8; font-size: 10px; }
  .legend span { display: inline-flex; align-items: center; gap: 4px; }
  .legend i { width: 7px; height: 7px; border-radius: 2px; }
  .empty { margin: 12px 0; color: #90959d; font-size: 12px; }
</style>
