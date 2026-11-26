<script lang="ts">
  import HistoryView from './components/HistoryView.svelte'
  import WidgetView from './components/WidgetView.svelte'

  const isHistory = location.hash === '#history'
</script>

<div class="app-shell">
  {#if isHistory}
    <HistoryView />
  {:else}
    <WidgetView />
  {/if}
</div>

<style>
  .app-shell { width: 100%; height: 100%; overflow: hidden; }
</style>
