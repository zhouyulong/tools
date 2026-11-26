type TimerHandle = unknown

type IdleFadeOptions = {
  idleMs?: number
  fadeOpacity?: number
  schedule?: (callback: () => void, delayMs: number) => TimerHandle
  cancel?: (handle: TimerHandle) => void
}

const interactionEvents = ['mouseenter', 'mousemove', 'mousedown', 'keydown', 'touchstart'] as const
const passiveListener = { passive: true }

export function createIdleFadeController({
  idleMs = 10_000,
  fadeOpacity = 0.2,
  schedule = (callback, delayMs) => globalThis.setTimeout(callback, delayMs),
  cancel = handle => globalThis.clearTimeout(handle as ReturnType<typeof globalThis.setTimeout>),
}: IdleFadeOptions = {}) {
  let element: HTMLElement | undefined
  let timer: TimerHandle | undefined
  let destroyed = false
  let previousOpacity = ''
  let previousTransition = ''

  const clearTimer = () => {
    if (timer === undefined) return
    cancel(timer)
    timer = undefined
  }

  const fade = () => {
    timer = undefined
    if (!destroyed && element) element.style.opacity = String(fadeOpacity)
  }

  const reset = () => {
    if (destroyed || !element) return
    clearTimer()
    element.style.opacity = '1'
    if (idleMs <= 0) fade()
    else timer = schedule(fade, idleMs)
  }

  const handleInteraction = () => reset()

  const unbind = () => {
    clearTimer()
    if (!element) return
    for (const event of interactionEvents) {
      element.removeEventListener(event, handleInteraction)
    }
    element.style.opacity = previousOpacity
    element.style.transition = previousTransition
    element = undefined
  }

  const bind = (nextElement: HTMLElement) => {
    if (destroyed) return
    if (element === nextElement) {
      reset()
      return
    }
    unbind()
    element = nextElement
    previousOpacity = element.style.opacity
    previousTransition = element.style.transition
    element.style.transition = 'opacity 300ms ease'
    for (const event of interactionEvents) {
      element.addEventListener(event, handleInteraction, passiveListener)
    }
    reset()
  }

  const destroy = () => {
    if (destroyed) return
    unbind()
    destroyed = true
  }

  return { bind, reset, destroy }
}
