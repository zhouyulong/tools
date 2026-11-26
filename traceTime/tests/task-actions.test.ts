import assert from 'node:assert/strict'
import { test } from 'node:test'
import { bindPausedTask } from '../src/lib/taskActions.ts'
import type { Task } from '../src/lib/api.ts'

const a: Task = { id: 'A', name: 'A', est_seconds: 60, status: 'paused', tag: null,
  net_seconds: 0, created_at: '2026-11-26T00:00:00Z', finished_at: null,
  paused_since: '2026-11-26T00:00:01Z' }

test('render-bound banner ignores finish, abandon and resume after polling changes the task before paint', async () => {
  let current: Task | null = a
  const bound = bindPausedTask(a.id, () => current)
  const calls: string[] = []
  // The old rendered banner still exists; its live source has already advanced to B.
  current = { ...a, id: 'B' }
  for (const kind of ['finish', 'abandon', 'resume']) {
    assert.equal(bound.isCurrent(), false)
    assert.equal(await bound.run(async id => { calls.push(`${kind}:${id}`); return a }), null)
  }
  assert.deepEqual(calls, [])
})

test('matching paused task passes the rendered id, but running or removed tasks are ignored', async () => {
  let current: Task | null = a
  const bound = bindPausedTask('A', () => current)
  const calls: string[] = []
  const action = async (id: string) => { calls.push(id); return a }
  assert.equal(bound.isCurrent(), true)
  assert.equal((await bound.run(action))?.id, 'A')
  current = { ...a, status: 'running' }
  assert.equal(await bound.run(action), null)
  current = null
  assert.equal(await bound.run(action), null)
  assert.deepEqual(calls, ['A'])
})
