<script lang="ts">
  import { onMount } from 'svelte'
  import { getSetting, setSetting } from '../lib/api'
  import {
    SETTINGS,
    defaultReminderSettings,
    normalizeReminderSettings,
    type ReminderSettingKey,
    type ReminderSettingValues,
  } from '../lib/settings'

  let values: ReminderSettingValues = defaultReminderSettings()
  let loading = true
  let saving = false
  let errors: string[] = []
  let saved = false

  const spec = (key: ReminderSettingKey) => SETTINGS.find(item => item.key === key)!

  function edit(key: ReminderSettingKey, value: string) {
    values = { ...values, [key]: value }
    saved = false
    errors = []
  }

  async function load() {
    loading = true
    errors = []
    try {
      const loaded = await Promise.all(SETTINGS.map(async item => [
        item.key,
        (await getSetting(item.key)) ?? item.default,
      ] as const))
      values = normalizeReminderSettings(Object.fromEntries(loaded)).values
    } catch (error) {
      errors = [`读取设置失败：${String(error)}`]
    } finally {
      loading = false
    }
  }

  async function save() {
    if (saving) return
    const normalized = normalizeReminderSettings(values)
    if (normalized.errors.length > 0) {
      errors = normalized.errors
      saved = false
      return
    }

    saving = true
    errors = []
    saved = false
    values = normalized.values
    try {
      await Promise.all(SETTINGS.map(item => setSetting(item.key, normalized.values[item.key])))
      saved = true
    } catch (error) {
      errors = [`保存失败：${String(error)}`]
    } finally {
      saving = false
    }
  }

  onMount(() => { void load() })
</script>

<main class="settings-view">
  <header>
    <div>
      <h1>提醒设置</h1>
      <p>修改后将用于后续提醒判定</p>
    </div>
    <button type="button" disabled={loading || saving} on:click={save}>保存</button>
  </header>

  {#if loading}
    <p class="state">加载中…</p>
  {:else}
    <section>
      <label class="switch-row">
        <span>{spec('reminder_enabled').label}</span>
        <input
          type="checkbox"
          checked={values.reminder_enabled === 'true'}
          on:change={(event) => edit('reminder_enabled', String(event.currentTarget.checked))}
        />
      </label>
    </section>

    <section>
      <h2>75% 提前预警</h2>
      <label class="switch-row">
        <span>视觉提醒</span>
        <input type="checkbox" checked={values.reminder_warn_enabled === 'true'} on:change={(event) => edit('reminder_warn_enabled', String(event.currentTarget.checked))} />
      </label>
      <label class="number-row">
        <span>触发百分比</span>
        <span><input aria-label="提前预警百分比" type="number" min="1" max="1000" step="1" value={values.reminder_warn_percent} on:input={(event) => edit('reminder_warn_percent', event.currentTarget.value)} /> %</span>
      </label>
      <label class="switch-row">
        <span>声音</span>
        <input type="checkbox" checked={values.reminder_warn_sound === 'true'} on:change={(event) => edit('reminder_warn_sound', String(event.currentTarget.checked))} />
      </label>
    </section>

    <section>
      <h2>90% 临近截止</h2>
      <label class="switch-row">
        <span>视觉提醒</span>
        <input type="checkbox" checked={values.reminder_near_enabled === 'true'} on:change={(event) => edit('reminder_near_enabled', String(event.currentTarget.checked))} />
      </label>
      <label class="number-row">
        <span>触发百分比</span>
        <span><input aria-label="临近截止百分比" type="number" min="1" max="1000" step="1" value={values.reminder_near_percent} on:input={(event) => edit('reminder_near_percent', event.currentTarget.value)} /> %</span>
      </label>
      <label class="switch-row">
        <span>声音</span>
        <input type="checkbox" checked={values.reminder_near_sound === 'true'} on:change={(event) => edit('reminder_near_sound', String(event.currentTarget.checked))} />
      </label>
    </section>

    <section>
      <h2>100% 到期</h2>
      <label class="switch-row">
        <span>视觉提醒</span>
        <input type="checkbox" checked={values.reminder_due_enabled === 'true'} on:change={(event) => edit('reminder_due_enabled', String(event.currentTarget.checked))} />
      </label>
      <label class="number-row">
        <span>触发百分比</span>
        <span><input aria-label="到期百分比" type="number" min="1" max="1000" step="1" value={values.reminder_due_percent} on:input={(event) => edit('reminder_due_percent', event.currentTarget.value)} /> %</span>
      </label>
      <label class="switch-row">
        <span>系统通知</span>
        <input type="checkbox" checked={values.reminder_due_notify === 'true'} on:change={(event) => edit('reminder_due_notify', String(event.currentTarget.checked))} />
      </label>
    </section>

    <section>
      <h2>超时</h2>
      <label class="switch-row">
        <span>启用超时提醒</span>
        <input type="checkbox" checked={values.reminder_overdue_enabled === 'true'} on:change={(event) => edit('reminder_overdue_enabled', String(event.currentTarget.checked))} />
      </label>
      <label class="switch-row">
        <span>系统通知</span>
        <input type="checkbox" checked={values.reminder_overdue_notify === 'true'} on:change={(event) => edit('reminder_overdue_notify', String(event.currentTarget.checked))} />
      </label>
      <label class="number-row">
        <span>重复周期</span>
        <span><input aria-label="超时重复周期" type="number" min="1" max="1440" step="1" value={values.reminder_overdue_interval_minutes} on:input={(event) => edit('reminder_overdue_interval_minutes', event.currentTarget.value)} /> 分钟</span>
      </label>
    </section>
  {/if}

  {#if errors.length > 0}
    <ul class="errors">
      {#each errors as error}<li>{error}</li>{/each}
    </ul>
  {:else if saved}
    <p class="saved">已保存</p>
  {/if}
</main>

<style>
  .settings-view { width: 100%; height: 100%; padding: 14px; overflow-y: auto; background: #1e1e22; color: #f0f0f0; }
  header { position: sticky; z-index: 1; top: -14px; display: flex; align-items: center; justify-content: space-between; padding: 12px 0 8px; background: #1e1e22; }
  h1 { margin: 0; font-size: 18px; }
  header p { margin: 3px 0 0; color: #9ca0a8; }
  header button { min-width: 58px; height: 28px; border: 1px solid #596173; border-radius: 5px; background: #343946; color: #fff; cursor: pointer; }
  header button:disabled { cursor: default; opacity: 0.5; }
  section { margin: 0 0 10px; padding: 9px 10px; border: 1px solid #383a40; border-radius: 7px; background: #27282d; }
  h2 { margin: 0 0 5px; font-size: 12px; color: #d8d9dc; }
  label { min-height: 24px; }
  .switch-row, .number-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .switch-row input { width: 16px; height: 16px; }
  .number-row input { width: 72px; height: 22px; padding: 1px 4px; }
  .state { color: #aaaeb5; }
  .errors { margin: 6px 0 0; padding-left: 18px; color: #ff8177; }
  .saved { margin: 6px 0 0; color: #7bd88f; font-weight: 600; }
</style>
