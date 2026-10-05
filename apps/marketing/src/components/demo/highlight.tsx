import type { ReactNode } from "react"

const KEYWORDS =
  "const|let|var|return|if|else|import|from|export|function|new|await|async|for|of|in|throw|type|interface|struct|func|guard|try|throws|self|class|extension|private|static|while|break"
const CONSTANTS = "null|undefined|true|false|nil"

const TOKEN = new RegExp(
  [
    String.raw`(\/\/.*$|#.*$)`,
    String.raw`("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|\x60[^\x60]*\x60)`,
    String.raw`\b(${KEYWORDS})\b`,
    String.raw`\b(${CONSTANTS}|\d+(?:\.\d+)?)\b`,
    String.raw`\b([A-Z][A-Za-z0-9]*)\b`,
    String.raw`\b([a-z_][A-Za-z0-9_]*)(?=\()`,
  ].join("|"),
  "gm"
)

const CLASSES = [
  "text-syntax-comment",
  "text-syntax-string",
  "text-syntax-keyword",
  "text-syntax-constant",
  "text-syntax-type",
  "text-syntax-function",
]

/** Colours a line of code the way the core's highlighter does, for the few languages the demo shows. */
export function highlight(line: string) {
  const parts: ReactNode[] = []
  let at = 0
  for (const match of line.matchAll(TOKEN)) {
    const group = match.slice(1).findIndex((part) => part !== undefined)
    if (match.index > at) parts.push(line.slice(at, match.index))
    parts.push(
      <span key={match.index} className={CLASSES[group]}>
        {match[0]}
      </span>
    )
    at = match.index + match[0].length
  }
  if (at < line.length) parts.push(line.slice(at))
  return parts
}
