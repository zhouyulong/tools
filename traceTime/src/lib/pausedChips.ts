import type { Task } from './api'

/** Two fixed-width columns prevent a whole chip from falling outside the wrapping row. */
export function pausedChipLayout(tasks: Task[], widgetHeight: number, hasError = false) {
  // Match WidgetRunning's border-box budget: padding 6 + head 14 + time 28 + foot 20.
  // Chips start after a 4px margin; two 18px rows have a 4px gap; an error uses 14px.
  const twoRowsHeight = 68 + 4 + 2 * 18 + 4 + (hasError ? 14 : 0)
  const maxRows = widgetHeight >= twoRowsHeight ? 2 : 1
  const capacity = maxRows * 2
  // Reserve the final slot for +N instead of rendering off-screen task elements.
  const visibleCount = tasks.length > capacity ? capacity - 1 : tasks.length
  const hiddenCount = tasks.length - visibleCount
  return {
    visible: tasks.slice(0, visibleCount),
    hiddenCount,
    rows: Math.ceil((visibleCount + (hiddenCount ? 1 : 0)) / 2),
    allNames: tasks.map(t => t.name).join('\n'),
  }
}
