import assert from 'node:assert/strict'
import { afterEach, test } from 'node:test'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { finishTask, abandonTask, resume } from '../src/lib/api.ts'

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
