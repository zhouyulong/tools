<script lang="ts">
  import HistoryView from './components/HistoryView.svelte'
  import WidgetView from './components/WidgetView.svelte'

  const isHistory = location.hash === '#history'
</script>

{#if isHistory}
  <HistoryView />
{:else}
  <WidgetView />
{/if}
