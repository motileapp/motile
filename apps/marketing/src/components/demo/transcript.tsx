import {
  CopyIcon,
  FileTextIcon,
  GlobeIcon,
  PencilIcon,
  SearchIcon,
  SquareTerminalIcon,
  type LucideIcon,
} from "lucide-react"
import type { ReactNode } from "react"
import { parseDiff } from "./diff"
import { highlight } from "./highlight"
import type { Item, Thread, Tool } from "./threads"
import { cn } from "@/lib/utils"

const TOOL_ICONS: Record<Tool, LucideIcon> = {
  search: SearchIcon,
  read: FileTextIcon,
  edit: PencilIcon,
  run: SquareTerminalIcon,
  check: GlobeIcon,
}

export function Transcript({
  thread,
  elapsed,
  onOpenDiff,
}: {
  thread: Thread
  elapsed: string
  onOpenDiff: () => void
}) {
  return (
    <div className="mx-auto flex max-w-[844px] flex-col px-[38px] pt-6">
      {thread.items.map((item, index) => (
        <Row key={index} item={item} thread={thread} onOpenDiff={onOpenDiff} />
      ))}
      {thread.status.kind === "working" && (
        <Shimmer className="mt-2 text-[13px] tabular-nums">
          Working for {elapsed}
        </Shimmer>
      )}
      {thread.status.kind === "approval" && (
        <p className="mt-2 text-[13px] text-activity">
          Waiting for your approval
        </p>
      )}
    </div>
  )
}

function Row({
  item,
  thread,
  onOpenDiff,
}: {
  item: Item
  thread: Thread
  onOpenDiff: () => void
}) {
  switch (item.kind) {
    case "user":
      return (
        <div className="mb-5 flex justify-end pt-1">
          <p className="max-w-[80%] rounded-[18px] bg-bubble px-3.5 py-2.5 text-[14px] leading-6 text-foreground">
            {item.text}
          </p>
        </div>
      )
    case "text":
      return <Prose text={item.text} />
    case "code":
      return (
        <div className="my-3 overflow-hidden rounded-[10px] bg-bubble">
          <div className="flex h-8 items-center justify-between px-3.5 text-[11.5px] text-tertiary">
            {item.lang}
            <CopyIcon className="size-3.5" />
          </div>
          <pre className="overflow-x-auto px-3.5 pb-3 font-mono text-[12.5px] leading-[18px] text-foreground">
            {item.code.split("\n").map((line, index) => (
              <div key={index}>{line ? highlight(line) : " "}</div>
            ))}
          </pre>
        </div>
      )
    case "tool": {
      const Icon = TOOL_ICONS[item.tool]
      return (
        <div className="-mx-2 flex h-7 items-center gap-2 rounded-md px-2 text-[13px] text-activity">
          <Icon className="size-3.5 shrink-0" />
          {item.running ? (
            <Shimmer>{item.verb}</Shimmer>
          ) : (
            <span>{item.verb}</span>
          )}
          <span className="truncate font-mono text-[12px] text-tertiary">
            {item.target}
          </span>
        </div>
      )
    }
    case "changes":
      return <Changes thread={thread} onOpenDiff={onOpenDiff} />
    case "end":
      return (
        <div className="mt-2 mb-4 flex h-7 items-center gap-2 pl-[7px] text-[12px] text-tertiary">
          {item.worked}
          <CopyIcon className="size-3" />
        </div>
      )
  }
}

export function Shimmer({
  children,
  className,
}: {
  children: ReactNode
  className?: string
}) {
  return (
    <span
      className={cn(
        "animate-[demo-shimmer_2s_linear_infinite] bg-[linear-gradient(90deg,var(--activity)_40%,var(--foreground)_50%,var(--activity)_60%)] bg-size-[200%_100%] bg-clip-text text-transparent",
        className
      )}
    >
      {children}
    </span>
  )
}

/** The bit of Markdown the replies use: paragraphs, headings, lists, `code` and **bold**. */
function Prose({ text }: { text: string }) {
  return (
    <div className="my-2 text-[14px] leading-6 text-prose">
      {text.split("\n\n").map((block, index) => {
        if (block.startsWith("### ")) {
          return (
            <h3
              key={index}
              className="mt-4 mb-1 text-[15px] font-semibold text-foreground"
            >
              {inline(block.slice(4).split("\n")[0])}
              {block.includes("\n") && (
                <List lines={block.split("\n").slice(1)} />
              )}
            </h3>
          )
        }
        if (block.startsWith("- "))
          return <List key={index} lines={block.split("\n")} />
        return (
          <p key={index} className="my-2">
            {inline(block)}
          </p>
        )
      })}
    </div>
  )
}

function List({ lines }: { lines: string[] }) {
  return (
    <ul className="mt-2 flex list-disc flex-col gap-1 pl-5 text-[14px] font-normal text-prose marker:text-tertiary">
      {lines.map((line, index) => (
        <li key={index}>{inline(line.slice(2))}</li>
      ))}
    </ul>
  )
}

function inline(text: string) {
  return text.split(/(`[^`]+`|\*\*[^*]+\*\*)/).map((part, index) => {
    if (part.startsWith("`")) {
      return (
        <code
          key={index}
          className="rounded-[5px] bg-bubble px-1 py-px font-mono text-[12.5px] text-foreground"
        >
          {part.slice(1, -1)}
        </code>
      )
    }
    if (part.startsWith("**")) {
      return (
        <strong key={index} className="font-semibold text-foreground">
          {part.slice(2, -2)}
        </strong>
      )
    }
    return part
  })
}

/** What the turn changed: its files and their lines, and a button that opens the diff. */
function Changes({
  thread,
  onOpenDiff,
}: {
  thread: Thread
  onOpenDiff: () => void
}) {
  const files = parseDiff(thread.diff)
  const added = files.reduce((sum, file) => sum + file.added, 0)
  const removed = files.reduce((sum, file) => sum + file.removed, 0)
  return (
    <div className="mt-3 mb-3 rounded-[10px] border bg-bubble pb-1.5">
      <div className="flex h-10 items-center justify-between pr-1.5 pl-3.5">
        <span className="text-[13px] font-medium text-foreground">
          {files.length} files changed{" "}
          <LineCounts added={added} removed={removed} />
        </span>
        <button
          type="button"
          onClick={onOpenDiff}
          className="h-7 rounded-md px-2 text-[12.5px] font-medium text-foreground hover:bg-hover"
        >
          Open diff
        </button>
      </div>
      {files.map((file) => (
        <button
          type="button"
          key={file.path}
          onClick={onOpenDiff}
          className="mx-1.5 flex h-[26px] w-[calc(100%-12px)] items-center gap-2 rounded-md px-1.5 text-left text-[12.5px] hover:bg-hover"
        >
          <FileTextIcon className="size-3 shrink-0 text-muted-foreground" />
          <span className="truncate text-foreground">
            <span className="text-muted-foreground">{folderOf(file.path)}</span>
            {nameOf(file.path)}
          </span>
          <span className="ml-auto">
            <LineCounts added={file.added} removed={file.removed} />
          </span>
        </button>
      ))}
    </div>
  )
}

export function LineCounts({
  added,
  removed,
}: {
  added: number
  removed: number
}) {
  return (
    <span className="font-mono text-[11.5px] font-normal tabular-nums">
      {added > 0 && <span className="text-success">+{added}</span>}
      {added > 0 && removed > 0 && " "}
      {removed > 0 && <span className="text-destructive">−{removed}</span>}
    </span>
  )
}

export function folderOf(path: string) {
  const slash = path.lastIndexOf("/")
  return slash < 0 ? "" : path.slice(0, slash + 1)
}

export function nameOf(path: string) {
  return path.slice(path.lastIndexOf("/") + 1)
}
