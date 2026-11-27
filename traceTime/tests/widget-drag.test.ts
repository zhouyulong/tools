import assert from 'node:assert/strict'
import test from 'node:test'

type FakeElement = {
  tagName: string
  closest: (selector: string) => FakeElement | null
}

function element(tagName: string, closest?: FakeElement | null): FakeElement {
  return { tagName, closest: closest === undefined ? () => null : () => closest }
}

test('left presses on empty or text areas start dragging', async () => {
  const { shouldStartWidgetDrag } = await import('../src/lib/widget-drag.ts')
  const name = element('STRONG')
  assert.equal(shouldStartWidgetDrag({ button: 0, target: name }), true)
  assert.equal(shouldStartWidgetDrag({ button: 0, target: element('MAIN') }), true)
})

test('controls, labels, links, and non-left presses do not drag', async () => {
  const { shouldStartWidgetDrag } = await import('../src/lib/widget-drag.ts')
  const control = element('BUTTON', element('BUTTON'))
  const input = element('INPUT', element('INPUT'))
  assert.equal(shouldStartWidgetDrag({ button: 0, target: control }), false)
  assert.equal(shouldStartWidgetDrag({ button: 0, target: input }), false)
  assert.equal(shouldStartWidgetDrag({ button: 2, target: element('MAIN') }), false)
})
