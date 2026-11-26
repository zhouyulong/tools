<script lang="ts">
  import HistoryView from './components/HistoryView.svelte'
  import SettingsView from './components/SettingsView.svelte'
  import WidgetView from './components/WidgetView.svelte'

  const isHistory = location.hash === '#history'
  const isSettings = location.hash === '#settings'
</script>

<div class="app-shell">
  {#if isHistory}
    <HistoryView />
  {:else if isSettings}
    <SettingsView />
  {:else}
    <WidgetView />
  {/if}
</div>

<style>
  .app-shell { width: 100%; height: 100%; overflow: hidden; }
</style>
