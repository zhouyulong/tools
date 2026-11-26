<script lang="ts">
  import { onMount } from 'svelte'
  import { applyBackgroundMode, getSetting, getWidgetBackgroundMode, saveSettings, type BackgroundModeResult } from '../lib/api'
  import {
    SETTINGS,
    defaultSettings,
    loadStoredSettings,
    normalizeSettings,
    type BackgroundMode,
    type ReminderSettingKey,
    type SettingKey,
    type SettingValues,
  } from '../lib/settings'

  const backgroundChoices: { value: BackgroundMode; label: string }[] = [
    { value: 'glass', label: '毛玻璃（默认）' },
    { value: 'translucent', label: '半透明纯色' },
    { value: 'opaque', label: '不透明' },
  ]

  let values: SettingValues = defaultSettings()
  let loading = true
  let saving = false
  let errors: string[] = []
  let saved = false
  let invalidStoredValues = false
  let settingsReadFailed = false
  let backgroundResult: BackgroundModeResult | undefined

  const spec = (key: ReminderSettingKey) => SETTINGS.find(item => item.key === key)!

  function edit(key: SettingKey, value: string) {
    if (saving) return
    values = { ...values, [key]: value } as SettingValues
    saved = false
    errors = []
  }

  async function load() {
    loading = true
    errors = []
    const [settings, background] = await Promise.all([
      loadStoredSettings(getSetting),
      getWidgetBackgroundMode().catch(() => undefined),
    ])
    values = settings.values
    invalidStoredValues = settings.hasInvalidValues
    settingsReadFailed = settings.readFailed
    backgroundResult = background
    loading = false
  }

  async function save() {
    if (saving) return
    const normalized = normalizeSettings(values)
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
      await saveSettings(normalized.values)
      invalidStoredValues = false
      settingsReadFailed = false
      saved = true
      try {
        backgroundResult = await applyBackgroundMode(normalized.values.background_mode)
      } catch (error) {
        errors = [`设置已保存，但未能应用背景：${String(error)}`]
      }
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
      <h1>设置</h1>
      <p>修改后将立即用于主窗口</p>
    </div>
    <button type="button" disabled={loading || saving} on:click={save}>保存</button>
  </header>

  {#if loading}
    <p class="state">加载中…</p>
  {:else}
    {#if invalidStoredValues}
      <p class="settings-note warning" role="status">部分设置无效，已显示默认值；保存后修复</p>
    {/if}
    {#if settingsReadFailed}
      <p class="settings-note warning" role="status">部分设置读取失败，已显示默认值</p>
    {/if}
    <section>
      <h2>背景</h2>
      <div class="choice-group">
        {#each backgroundChoices as choice}
          <label class="choice-row">
            <input
              type="radio"
              name="background-mode"
              value={choice.value}
              disabled={saving}
              checked={values.background_mode === choice.value}
              on:change={() => edit('background_mode', choice.value)}
            />
            <span>{choice.label}</span>
          </label>
        {/each}
      </div>
      {#if backgroundResult?.requested === 'glass' && backgroundResult.effective === 'translucent'}
        <p class="settings-note" role="status">当前实际：半透明纯色</p>
      {/if}
    </section>

    <section>
      <label class="switch-row">
        <span>{spec('reminder_enabled').label}</span>
        <input
          type="checkbox" disabled={saving}
          checked={values.reminder_enabled === 'true'}
          on:change={(event) => edit('reminder_enabled', String(event.currentTarget.checked))}
        />
      </label>
    </section>

    <section>
      <h2>75% 提前预警</h2>
      <label class="switch-row">
        <span>视觉提醒</span>
        <input type="checkbox" disabled={saving} checked={values.reminder_warn_enabled === 'true'} on:change={(event) => edit('reminder_warn_enabled', String(event.currentTarget.checked))} />
      </label>
      <label class="number-row">
        <span>触发百分比</span>
        <span><input aria-label="提前预警百分比" type="number" disabled={saving} min="1" max="1000" step="1" value={values.reminder_warn_percent} on:input={(event) => edit('reminder_warn_percent', event.currentTarget.value)} /> %</span>
      </label>
      <label class="switch-row">
        <span>声音</span>
        <input type="checkbox" disabled={saving} checked={values.reminder_warn_sound === 'true'} on:change={(event) => edit('reminder_warn_sound', String(event.currentTarget.checked))} />
      </label>
    </section>

    <section>
      <h2>90% 临近截止</h2>
      <label class="switch-row">
        <span>视觉提醒</span>
        <input type="checkbox" disabled={saving} checked={values.reminder_near_enabled === 'true'} on:change={(event) => edit('reminder_near_enabled', String(event.currentTarget.checked))} />
      </label>
      <label class="number-row">
        <span>触发百分比</span>
        <span><input aria-label="临近截止百分比" type="number" disabled={saving} min="1" max="1000" step="1" value={values.reminder_near_percent} on:input={(event) => edit('reminder_near_percent', event.currentTarget.value)} /> %</span>
      </label>
      <label class="switch-row">
        <span>声音</span>
        <input type="checkbox" disabled={saving} checked={values.reminder_near_sound === 'true'} on:change={(event) => edit('reminder_near_sound', String(event.currentTarget.checked))} />
      </label>
    </section>

    <section>
      <h2>100% 到期</h2>
      <label class="switch-row">
        <span>视觉提醒</span>
        <input type="checkbox" disabled={saving} checked={values.reminder_due_enabled === 'true'} on:change={(event) => edit('reminder_due_enabled', String(event.currentTarget.checked))} />
      </label>
      <label class="number-row">
        <span>触发百分比</span>
        <span><input aria-label="到期百分比" type="number" disabled={saving} min="1" max="1000" step="1" value={values.reminder_due_percent} on:input={(event) => edit('reminder_due_percent', event.currentTarget.value)} /> %</span>
      </label>
      <label class="switch-row">
        <span>系统通知</span>
        <input type="checkbox" disabled={saving} checked={values.reminder_due_notify === 'true'} on:change={(event) => edit('reminder_due_notify', String(event.currentTarget.checked))} />
      </label>
    </section>

    <section>
      <h2>超时</h2>
      <label class="switch-row">
        <span>启用超时提醒</span>
        <input type="checkbox" disabled={saving} checked={values.reminder_overdue_enabled === 'true'} on:change={(event) => edit('reminder_overdue_enabled', String(event.currentTarget.checked))} />
      </label>
      <label class="switch-row">
        <span>系统通知</span>
        <input type="checkbox" disabled={saving} checked={values.reminder_overdue_notify === 'true'} on:change={(event) => edit('reminder_overdue_notify', String(event.currentTarget.checked))} />
      </label>
      <label class="number-row">
        <span>重复周期</span>
        <span><input aria-label="超时重复周期" type="number" disabled={saving} min="1" max="1440" step="1" value={values.reminder_overdue_interval_minutes} on:input={(event) => edit('reminder_overdue_interval_minutes', event.currentTarget.value)} /> 分钟</span>
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
  .choice-group { display: grid; gap: 3px; }
  .choice-row { display: flex; align-items: center; gap: 7px; cursor: pointer; }
  .choice-row input { width: 14px; height: 14px; margin: 0; }
  .state { color: #aaaeb5; }
  .settings-note { margin: 6px 0 0; font-size: 12px; color: #b9bbc2; }
  .warning { margin-bottom: 8px; color: #f3c272; }
  .errors { margin: 6px 0 0; padding-left: 18px; color: #ff8177; }
  .saved { margin: 6px 0 0; color: #7bd88f; font-weight: 600; }
</style>
