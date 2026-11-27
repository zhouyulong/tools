type DragPressEvent = {
  button: number
  target?: unknown
}

type DragTarget = {
  closest?: (selector: string) => unknown
}

const CONTROL_SELECTOR = [
  'button',
  'input',
  'select',
  'textarea',
  'label',
  'a',
  '[contenteditable="true"]',
  '[data-no-drag]',
].join(',')

/** Empty areas and task text can drag; every real control keeps its normal mouse behavior. */
export function shouldStartWidgetDrag(event: DragPressEvent): boolean {
  if (event.button !== 0) return false
  const target = event.target as DragTarget | null | undefined
  if (!target || typeof target.closest !== 'function') return false
  return target.closest(CONTROL_SELECTOR) === null
}
