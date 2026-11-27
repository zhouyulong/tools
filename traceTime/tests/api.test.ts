import assert from 'node:assert/strict'
import { afterEach, test } from 'node:test'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { applyBackgroundMode, getWidgetBackgroundMode, setWidgetIdleFade, finishTask, abandonTask, resume, deleteTask, deleteTasks, saveSettings, sendReminderNotification, setWidgetHeight, setIdleFadeOpacity } from '../src/lib/api.ts'

globalThis.window = {} as Window & typeof globalThis
afterEach(clearMocks)

for (const [command, action] of [
  ['finish_task', finishTask], ['abandon_task', abandonTask], ['resume', resume],
] as const) {
  test(`${command} binds the clicked task id and propagates a stale-task rejection`, async () => {
    const calls: unknown[] = []
    mockIPC((cmd, payload) => {
      calls.push([cmd, payload])
      return Promise.reject('task changed, please refresh')
    })
    await assert.rejects(action('child-B'), e => e === 'task changed, please refresh')
    assert.deepEqual(calls, [[command, { expectedTaskId: 'child-B' }]])
  })
}


test('soft deletion and resizing call registered commands with the explicit id/height', async () => {
  const calls: unknown[] = []
  mockIPC((command, payload) => { calls.push([command, payload]) })
  await deleteTask('finished-A')
  await deleteTasks(['A', 'B'])
  await setWidgetHeight(180)
  assert.deepEqual(calls, [
    ['delete_task', { taskId: 'finished-A' }],
    ['delete_tasks', { ids: ['A', 'B'] }],
    ['set_widget_height', { height: 180 }],
  ])
})

test('batch settings save uses one IPC command with the complete values map', async () => {
  const calls: unknown[] = []
  mockIPC((command, payload) => { calls.push([command, payload]) })
  const values = { reminder_enabled: 'true', reminder_warn_percent: '75' }
  await saveSettings(values)
  assert.deepEqual(calls, [['set_settings', { values }]])
})


test('reminder notification IPC includes its task, persisted node and level', async () => {
  const calls: unknown[] = []
  mockIPC((command, payload) => { calls.push([command, payload]) })
  await sendReminderNotification('task-B', 'overdue:at:600', 'overdue', 'Title', 'Body')
  assert.deepEqual(calls, [['send_reminder_notification', {
    taskId: 'task-B', node: 'overdue:at:600', level: 'overdue', title: 'Title', body: 'Body',
  }]])
})

test('background mode IPC returns requested and effective modes', async () => {
  const calls: unknown[] = []
  mockIPC((command, payload) => {
    calls.push([command, payload])
    return { requested: 'glass', effective: 'translucent', backgroundOpacity: 65 }
  })

  assert.deepEqual(await applyBackgroundMode('glass', 65), {
    requested: 'glass',
    effective: 'translucent',
    backgroundOpacity: 65,
  })
  assert.deepEqual(calls, [['apply_background_mode', { mode: 'glass', opacity: 65 }]])
})


test('background startup reads the actual mode and forwards both idle transitions', async () => {
  const calls: unknown[] = []
  mockIPC((command, payload) => {
    calls.push([command, payload])
    if (command === 'get_widget_background_mode') return { requested: 'glass', effective: 'translucent', backgroundOpacity: 65 }
  })
  assert.deepEqual(await getWidgetBackgroundMode(), { requested: 'glass', effective: 'translucent', backgroundOpacity: 65 })
  await setWidgetIdleFade(true)
  await setWidgetIdleFade(false)
  assert.deepEqual(calls, [
    ['get_widget_background_mode', {}],
    ['set_widget_idle_fade', { faded: true }],
    ['set_widget_idle_fade', { faded: false }],
  ])
})


test('idle fade opacity IPC forwards the configured percentage', async () => {
  const calls: unknown[] = []
  mockIPC((command, payload) => { calls.push([command, payload]) })
  await setIdleFadeOpacity(60)
  assert.deepEqual(calls, [['set_idle_fade_opacity', { opacity: 60 }]])
})
