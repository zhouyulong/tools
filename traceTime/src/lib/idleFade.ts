type TimerHandle = unknown

type IdleFadeOptions = {
  idleMs?: number
  fadeOpacity?: number
  onFadedChange?: (faded: boolean) => void | Promise<void>
  schedule?: (callback: () => void, delayMs: number) => TimerHandle
  cancel?: (handle: TimerHandle) => void
}

const interactionEvents = ['mouseenter', 'mousemove', 'mousedown', 'keydown', 'touchstart'] as const
const passiveListener = { passive: true }

export function createIdleFadeController({
  idleMs = 10_000,
  fadeOpacity = 0.2,
  onFadedChange,
  schedule = (callback, delayMs) => globalThis.setTimeout(callback, delayMs),
  cancel = handle => globalThis.clearTimeout(handle as ReturnType<typeof globalThis.setTimeout>),
}: IdleFadeOptions = {}) {
  let element: HTMLElement | undefined
  let timer: TimerHandle | undefined
  let destroyed = false
  let previousOpacity = ''
  let previousTransition = ''
  let faded = false
  let nativeTransition = Promise.resolve()

  const setFaded = (value: boolean) => {
    if (faded === value) return
    faded = value
    if (element) element.style.transition = value ? 'opacity 300ms ease' : 'none'
    // Serialize native updates; failures never roll back the immediate DOM change.
    if (onFadedChange) nativeTransition = nativeTransition
      .then(() => onFadedChange(value))
      .catch(() => {})
  }

  const clearTimer = () => {
    if (timer === undefined) return
    cancel(timer)
    timer = undefined
  }

  const fade = () => {
    timer = undefined
    if (!destroyed && element) {
      setFaded(true)
      element.style.opacity = String(fadeOpacity)
    }
  }

  const reset = () => {
    if (destroyed || !element) return
    clearTimer()
    setFaded(false)
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
    setFaded(false)
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
