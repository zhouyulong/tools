import assert from 'node:assert/strict'
import { test } from 'node:test'
import { pausedChipLayout } from '../src/lib/pausedChips.ts'
import type { Task } from '../src/lib/api.ts'

const tasks: Task[] = Array.from({ length: 7 }, (_, i) => ({
  id: String(i), name: `任务${i + 1}`, est_seconds: 60, status: 'paused', tag: null,
  net_seconds: 0, created_at: '2026-11-26T00:00:00Z', finished_at: null,
  paused_since: '2026-11-26T00:00:01Z',
}))

test('two-row layout reserves its last slot for the exact hidden-task count', () => {
  const result = pausedChipLayout(tasks, 160)
  assert.deepEqual(result.visible.map(t => t.name), ['任务1', '任务2', '任务3'])
  assert.equal(result.hiddenCount, 4)
  assert.equal(result.rows, 2)
  assert.equal(result.allNames, '任务1\n任务2\n任务3\n任务4\n任务5\n任务6\n任务7')
})

test('list growth, shrink and reorder recompute rows and collapse count without mutation', () => {
  const before = structuredClone(tasks)
  assert.equal(pausedChipLayout([], 160).rows, 0)
  for (const [count, rows] of [[1, 1], [2, 1], [3, 2], [4, 2]]) {
    const result = pausedChipLayout(tasks.slice(0, count), 160)
    assert.equal(result.visible.length, count)
    assert.equal(result.hiddenCount, 0)
    assert.equal(result.rows, rows)
  }
  assert.equal(pausedChipLayout(tasks.slice(0, 5), 160).hiddenCount, 2)
  assert.equal(pausedChipLayout([...tasks].reverse(), 160).visible[0].name, '任务7')
  assert.deepEqual(tasks, before)
})

test('a constrained widget keeps the collapse marker in the first fully visible row', () => {
  const result = pausedChipLayout(tasks, 105)
  assert.deepEqual(result.visible.map(t => t.name), ['任务1'])
  assert.equal(result.hiddenCount, 6)
  assert.equal(result.rows, 1)
  // Until the client-height binding has measured the widget, stay conservative.
  assert.equal(pausedChipLayout(tasks, 0).rows, 1)
})

test('row budget reacts to height and error text before a second row can be clipped', () => {
  assert.equal(pausedChipLayout(tasks, 111).rows, 1)
  assert.equal(pausedChipLayout(tasks, 112).rows, 2)
  assert.equal(pausedChipLayout(tasks, 125, true).rows, 1)
  assert.equal(pausedChipLayout(tasks, 126, true).rows, 2)
})
