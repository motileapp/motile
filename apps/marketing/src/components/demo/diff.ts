export type DiffLine = {
  kind: "added" | "removed" | "unchanged" | "note"
  text: string
  old?: number
  new?: number
}

export type DiffFile = {
  path: string
  isNew: boolean
  added: number
  removed: number
  lines: DiffLine[]
}

/** Reads the demo's diffs: a file's path (with "new" after it if it was added), then its hunks. */
export function parseDiff(diff: string): DiffFile[] {
  const files: DiffFile[] = []
  let oldLine = 0
  let newLine = 0
  for (const line of diff.split("\n")) {
    const file = files.at(-1)
    const hunk = line.match(/^@@ -(\d+),?\d* \+(\d+),?\d* @@/)
    if (hunk && file) {
      oldLine = Number(hunk[1])
      newLine = Number(hunk[2])
      if (file.lines.length > 0) file.lines.push({ kind: "note", text: "⋯" })
      continue
    }
    const sign = line[0]
    if (sign === "+" && file) {
      file.lines.push({ kind: "added", text: line.slice(1), new: newLine++ })
      file.added++
    } else if (sign === "-" && file) {
      file.lines.push({ kind: "removed", text: line.slice(1), old: oldLine++ })
      file.removed++
    } else if ((sign === " " || line === "") && file) {
      file.lines.push({
        kind: "unchanged",
        text: line.slice(1),
        old: oldLine++,
        new: newLine++,
      })
    } else if (line) {
      const [path, note] = line.split(" ")
      files.push({
        path,
        isNew: note === "new",
        added: 0,
        removed: 0,
        lines: [],
      })
    }
  }
  return files
}
