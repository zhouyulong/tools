import type { Task } from './api'

/** Bind actions to one rendered banner, while checking the live polling source at click time. */
export function bindPausedTask(renderedTaskId: string, getCurrentTask: () => Task | null) {
  function isCurrent() {
    const current = getCurrentTask()
    return current?.id === renderedTaskId && current.status === 'paused'
  }
  return {
    isCurrent,
    async run(action: (expectedTaskId: string) => Promise<Task>): Promise<Task | null> {
      if (!isCurrent()) return null
      return action(renderedTaskId)
    },
  }
}
