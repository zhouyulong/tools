/** Observe intrinsic content, never the window-sized shell, to avoid resize feedback. */
export function observeWidgetHeight(
  element: HTMLElement,
  resize: (height: number) => Promise<void>,
  Observer: new (callback: () => void) => Pick<ResizeObserver, 'observe' | 'disconnect'> = ResizeObserver,
  delay = 100,
): () => void {
  let timer: ReturnType<typeof setTimeout> | undefined
  let lastHeight: number | undefined
  let active = true
  const changed = () => {
    clearTimeout(timer)
    timer = setTimeout(() => {
      const height = Math.min(220, Math.max(140, element.offsetHeight))
      if (!active || height === lastHeight) return
      lastHeight = height
      void resize(height).catch(error => {
        if (active && lastHeight === height) lastHeight = undefined
        console.warn('Unable to resize widget', error)
      })
    }, delay)
  }
  const observer = new Observer(changed)
  observer.observe(element)
  changed()
  return () => {
    active = false
    clearTimeout(timer)
    observer.disconnect()
  }
}
