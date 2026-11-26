type Rect = { x: number; y: number; width: number; height: number }

/** The springs and the curve Messages throws a sent message with. */
const travel = spring(1, 141.759, 17.3503)
const shrink = spring(2, 310, 38)
const regrow = spring(2, 320, 38)
const reshape = cubic(0.542, 0, 0.58, 1)
const LIFT = 0.055
const REGROW_AFTER = 0.185
const RESHAPE_TIME = 0.2
const DURATION = 1
/** The bubble's text is this far from its sides and from its top and bottom. */
const INSET = { x: 14, y: 10 }

/** Where the text of the field is in the demo's window, as tall as its lines. */
export function textRect(
  within: HTMLElement,
  field: HTMLTextAreaElement
): Rect {
  const style = getComputedStyle(field)
  const left = parseFloat(style.paddingLeft)
  const top = parseFloat(style.paddingTop)
  const width = field.clientWidth - left - parseFloat(style.paddingRight)
  const measure = document.createElement("div")
  Object.assign(measure.style, {
    position: "absolute",
    visibility: "hidden",
    width: `${width}px`,
    font: style.font,
    lineHeight: style.lineHeight,
    whiteSpace: "pre-wrap",
    overflowWrap: "break-word",
  })
  measure.textContent = field.value
  field.parentElement?.append(measure)
  const height = measure.offsetHeight
  measure.remove()
  const box = rectIn(within, field)
  return { x: box.x + left, y: box.y + top, width, height }
}

/** Where an element is in the demo's window, which the page may show scaled. */
export function rectIn(within: HTMLElement, element: Element): Rect {
  const outer = within.getBoundingClientRect()
  const inner = element.getBoundingClientRect()
  const scale = outer.width / within.offsetWidth || 1
  return {
    x: (inner.left - outer.left) / scale,
    y: (inner.top - outer.top) / scale,
    width: inner.width / scale,
    height: inner.height / scale,
  }
}

/**
 * Throws a sent message from where it was written to its bubble, as the app does: a copy of the
 * bubble starts around the text and takes the bubble's shape, travels to it on a spring that
 * leaves a moment before the one that lifts it, so that it curves, and shrinks as it leaves and
 * grows back as it lands. The bubble is hidden until then, and the transcript rises by `rise`
 * with it. The field's placeholder comes back once the copy has left it. Says how to stop it.
 */
export function throwMessage({
  within,
  text,
  from,
  field,
  bubble,
  transcript,
  rise,
}: {
  within: HTMLElement
  text: string
  from: Rect
  field: HTMLTextAreaElement
  bubble: HTMLElement
  transcript: HTMLElement
  rise: number
}) {
  field.dataset.thrown = ""
  const placeholder = setTimeout(() => delete field.dataset.thrown, 150)
  const final = rectIn(within, bubble)
  const start = {
    x: from.x - INSET.x,
    y: from.y - INSET.y,
    width: from.width + INSET.x * 2,
    height: from.height + INSET.y * 2,
  }
  const smallest = Math.min(
    0.9,
    Math.max(0.7, 0.7 + (start.height - 40.25) * 0.000827)
  )
  const copy = document.createElement("div")
  copy.setAttribute("aria-hidden", "true")
  copy.className =
    "pointer-events-none absolute top-0 left-0 z-20 origin-right px-3.5 py-2.5 text-(length:--type-md) leading-6 text-foreground"
  const fill = document.createElement("div")
  fill.className = "absolute inset-0 rounded-xl bg-background-secondary"
  const words = document.createElement("span")
  words.className = "relative block"
  words.style.width = `${final.width - INSET.x * 2}px`
  words.textContent = text
  copy.append(fill, words)
  within.append(copy)
  bubble.style.visibility = "hidden"

  let elapsed = 0
  let last: number | undefined
  let frame = 0
  const stop = () => {
    cancelAnimationFrame(frame)
    clearTimeout(placeholder)
    delete field.dataset.thrown
    copy.remove()
    bubble.style.visibility = ""
    transcript.style.transform = ""
  }
  const place = () => {
    transcript.style.transform = rise
      ? `translateY(${rise * (1 - travel(elapsed - LIFT))}px)`
      : ""
    const target = rectIn(within, bubble)
    const shaped = reshape(elapsed / RESHAPE_TIME)
    const width = start.width + (target.width - start.width) * shaped
    const height = start.height + (target.height - start.height) * shaped
    const startRight = start.x + start.width
    const right =
      startRight + (target.x + target.width - startRight) * travel(elapsed)
    const startMiddle = start.y + start.height / 2
    const middle =
      startMiddle +
      (target.y + target.height / 2 - startMiddle) * travel(elapsed - LIFT)
    const scale =
      (1 + (smallest - 1) * shrink(elapsed)) *
      (1 + (1 / smallest - 1) * regrow(elapsed - REGROW_AFTER))
    copy.style.width = `${width}px`
    copy.style.height = `${height}px`
    copy.style.transform = `translate(${right - width}px, ${middle - height / 2}px) scale(${scale})`
    fill.style.opacity = `${shaped}`
  }
  const step = (now: number) => {
    if (last !== undefined) elapsed += Math.min((now - last) / 1000, 1 / 30)
    last = now
    if (elapsed >= DURATION) return stop()
    place()
    frame = requestAnimationFrame(step)
  }
  place()
  frame = requestAnimationFrame(step)
  return stop
}

/** How far a spring from rest is `time` seconds in, as Core Animation runs one. */
function spring(mass: number, stiffness: number, damping: number) {
  const natural = Math.sqrt(stiffness / mass)
  const ratio = damping / (2 * Math.sqrt(stiffness * mass))
  const damped = natural * Math.sqrt(1 - ratio * ratio)
  return (time: number) => {
    if (time <= 0) return 0
    const decay = Math.exp(-ratio * natural * time)
    return (
      1 -
      decay *
        (Math.cos(damped * time) +
          ((ratio * natural) / damped) * Math.sin(damped * time))
    )
  }
}

/** A timing curve as `cubic-bezier()` draws it. */
function cubic(x1: number, y1: number, x2: number, y2: number) {
  const bezier = (t: number, a: number, b: number) =>
    3 * (1 - t) * (1 - t) * t * a + 3 * (1 - t) * t * t * b + t * t * t
  return (x: number) => {
    if (x <= 0) return 0
    if (x >= 1) return 1
    let [low, high] = [0, 1]
    for (let i = 0; i < 24; i++) {
      const middle = (low + high) / 2
      if (bezier(middle, x1, x2) < x) low = middle
      else high = middle
    }
    return bezier((low + high) / 2, y1, y2)
  }
}
