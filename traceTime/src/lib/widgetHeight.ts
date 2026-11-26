/** Observe intrinsic content, never the window-sized shell, to avoid resize feedback. */
export function createWidgetHeightObserver(
  element: HTMLElement,
  resize: (height: number) => Promise<void>,
  Observer: new (callback: () => void) => Pick<ResizeObserver, 'observe' | 'disconnect'> = ResizeObserver,
  delay = 100,
): { measureNow: () => void; cleanup: () => void } {
  let timer: ReturnType<typeof setTimeout> | undefined
  let lastHeight: number | undefined
  let active = true

  const changed = (immediate = false) => {
    clearTimeout(timer)
    const run = () => {
      // scrollHeight stays the complete content height when overflow:hidden clips the box.
      const height = Math.min(240, Math.max(140, element.scrollHeight))
      if (!active || height === lastHeight) return
      lastHeight = height
      void resize(height).catch(error => {
        if (active && lastHeight === height) lastHeight = undefined
        console.warn('Unable to resize widget', error)
      })
    }
    if (immediate) run()
    else timer = setTimeout(run, delay)
  }

  const observer = new Observer(() => changed())
  observer.observe(element)
  changed(true)
  return {
    measureNow: () => changed(true),
    cleanup: () => {
      active = false
      clearTimeout(timer)
      observer.disconnect()
    },
  }
}
