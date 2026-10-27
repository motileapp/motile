import {
  ArrowRightIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  CopyIcon,
  FileTextIcon,
  GlobeIcon,
  PencilIcon,
  SearchIcon,
  SquareTerminalIcon,
  type LucideIcon,
} from "lucide-react"
import { Fragment, useState, type ReactNode } from "react"
import { parseDiff } from "./diff"
import { highlight } from "./highlight"
import { AgentAvatar, AgentIcon } from "./icons"
import { SavingsProgram } from "./program"
import type {
  AgentState,
  HandoffEnd,
  Item,
  StartedAgent,
  Thread,
  Tool,
} from "./threads"
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
  elapsed: (since: number) => string
  onOpenDiff: () => void
}) {
  const [opened, setOpened] = useState<number[]>([])
  const turnFolds = folds(thread.items)
  const foldAt = (index: number) =>
    turnFolds.find((fold) => fold.from <= index && index <= fold.end)
  return (
    <div className="mx-auto flex max-w-[844px] flex-col px-[38px] pt-6">
      {thread.items.map((item, index) => {
        const fold = foldAt(index)
        const open = fold !== undefined && opened.includes(fold.from)
        const hidden =
          fold !== undefined &&
          !open &&
          index < fold.answer &&
          item.kind !== "agents"
        return (
          <Fragment key={index}>
            {fold?.from === index && (
              <FoldRow
                worked={fold.worked}
                open={open}
                onToggle={() =>
                  setOpened(
                    open
                      ? opened.filter((from) => from !== fold.from)
                      : [...opened, fold.from]
                  )
                }
              />
            )}
            {!hidden &&
              (item.kind === "end" && fold ? (
                <TurnEnd />
              ) : (
                <Row
                  item={item}
                  thread={thread}
                  elapsed={elapsed}
                  onOpenDiff={onOpenDiff}
                />
              ))}
          </Fragment>
        )
      })}
      {thread.status.kind === "working" && (
        <Shimmer className="mt-2 self-start text-[13px] tabular-nums">
          Working for {elapsed(thread.status.since)}
        </Shimmer>
      )}
      {thread.status.kind === "approval" && (
        <p className="mt-2 text-[13px] text-muted-stronger-foreground">
          Waiting for your approval
        </p>
      )}
    </div>
  )
}

function Row({
  item,
  thread,
  elapsed,
  onOpenDiff,
}: {
  item: Item
  thread: Thread
  elapsed: (since: number) => string
  onOpenDiff: () => void
}) {
  switch (item.kind) {
    case "user":
      return (
        <div className="mb-5 flex justify-end pt-1">
          <p className="max-w-[80%] rounded-xl bg-background-secondary px-3.5 py-2.5 text-[14px] leading-6 text-foreground">
            {item.text}
          </p>
        </div>
      )
    case "text":
      return <Prose text={item.text} />
    case "code":
      return (
        <div className="my-3 overflow-hidden rounded-lg bg-background-secondary">
          <div className="flex h-8 items-center justify-between px-3.5 text-[11.5px] text-muted-stronger-foreground">
            {item.lang}
            <CopyIcon className="size-3" />
          </div>
          <pre className="overflow-x-auto px-3.5 pb-3 font-mono text-[12.5px] leading-[18px] text-foreground">
            {item.code.split("\n").map((line, index) => (
              <div key={index}>{line ? highlight(line) : " "}</div>
            ))}
          </pre>
        </div>
      )
    case "ui":
      return <SavingsProgram />
    case "tool": {
      const Icon = TOOL_ICONS[item.tool]
      return (
        <div className="-mx-2 flex h-7 items-center gap-2 rounded-md px-2 text-[13px] text-muted-stronger-foreground">
          <Icon className="size-3.5 shrink-0" />
          {item.running ? (
            <Shimmer className="truncate">
              {item.verb}{" "}
              <span className="font-mono text-[12px]">{item.target}</span>
            </Shimmer>
          ) : (
            <span className="truncate">
              {item.verb}{" "}
              <span className="font-mono text-[12px] text-muted-foreground">
                {item.target}
              </span>
            </span>
          )}
        </div>
      )
    }
    case "changes":
      return <Changes thread={thread} onOpenDiff={onOpenDiff} />
    case "end":
      return <TurnEnd worked={item.worked} />
    case "agents":
      return item.agents.length === 1 ? (
        <AgentRow run={item.agents[0]} elapsed={elapsed} />
      ) : (
        <AgentGroup agents={item.agents} elapsed={elapsed} />
      )
    case "notice":
      return (
        <AgentRow
          run={item.agent}
          elapsed={elapsed}
          told={{ event: item.event, at: item.at }}
        />
      )
    case "handoff":
      return (
        <div className="flex h-9 items-center text-[12px] text-muted-stronger-foreground">
          <span className="h-px flex-1 bg-border" />
          <span className="mx-[11px] flex items-center gap-2">
            <Handoff end={item.from} />
            <ArrowRightIcon className="size-3" />
            <Handoff end={item.to} />
          </span>
          <span className="h-px flex-1 bg-border" />
        </div>
      )
  }
}

/**
 * A finished turn, from its first item after the message that started it to its end: what led up
 * to its answer folds behind one row, but for the agents it started.
 */
type Fold = { from: number; answer: number; end: number; worked: string }

const ANSWER_KINDS: Item["kind"][] = ["text", "code", "ui"]

function folds(items: Item[]): Fold[] {
  const found: Fold[] = []
  let from = 0
  items.forEach((item, end) => {
    if (["user", "notice", "handoff"].includes(item.kind)) from = end + 1
    if (item.kind !== "end") return
    let answer = end
    while (answer > from && !ANSWER_KINDS.includes(items[answer - 1].kind))
      answer -= 1
    while (answer > from && ANSWER_KINDS.includes(items[answer - 1].kind))
      answer -= 1
    if (answer > from) found.push({ from, answer, end, worked: item.worked })
    from = end + 1
  })
  return found
}

/** How long a finished turn worked, which opens what it did: it heads the turn, over a rule. */
function FoldRow({
  worked,
  open,
  onToggle,
}: {
  worked: string
  open: boolean
  onToggle: () => void
}) {
  const Chevron = open ? ChevronDownIcon : ChevronRightIcon
  return (
    <div className="flex flex-col">
      <button
        type="button"
        onClick={onToggle}
        className="-mx-2 flex h-7 items-center gap-2 self-start rounded-md px-2 text-[13px] text-muted-stronger-foreground hover:bg-background-accent-larger"
      >
        {worked}
        <Chevron className="-ml-1 size-3 shrink-0" />
      </button>
      <span className="mt-1 h-px bg-border" />
    </div>
  )
}

/** The line under a turn's answer; how long it worked, unless its fold says so. */
function TurnEnd({ worked }: { worked?: string }) {
  return (
    <div className="mt-2 mb-4 flex h-7 items-center gap-2 pl-[7px] text-[12px] text-muted-stronger-foreground">
      {worked}
      <CopyIcon className="size-3" />
    </div>
  )
}

const STATE_WORDS: Record<AgentState, string> = {
  working: "Working",
  waiting: "Needs approval",
  done: "Done",
  failed: "Failed",
}

const STATE_TEXT: Record<AgentState, string> = {
  working: "text-process",
  waiting: "text-warning",
  done: "text-success",
  failed: "text-destructive",
}

/**
 * An agent the thread's agent started, or what it was told of one: its avatar, its title and
 * word, what it runs on and does or said, and how long it worked. In the card of an open group it
 * lies on the card.
 */
function AgentRow({
  run,
  elapsed,
  told,
  card = false,
}: {
  run: StartedAgent
  elapsed: (since: number) => string
  told?: { event: string; at: string }
  card?: boolean
}) {
  const flagged = told || run.state === "waiting" || run.state === "failed"
  const working = run.state === "working" || run.state === "waiting"
  const time =
    told?.at ??
    (working && run.since !== undefined ? elapsed(run.since) : run.took)
  const line = told
    ? run.detail
    : [run.runsOn, run.detail].filter(Boolean).join(" · ")
  return (
    <button
      type="button"
      className={cn(
        "flex h-[50px] items-center gap-2.5 rounded-md text-left",
        card
          ? "w-full px-2 hover:bg-card-accent"
          : "-mx-1.5 w-[calc(100%+12px)] px-1.5 hover:bg-background-accent-larger"
      )}
    >
      <AgentAvatar agent={run.agent} state={told ? "done" : run.state} />
      <span className="min-w-0 flex-1">
        <span className="flex items-baseline gap-1.5">
          <span className="truncate text-[13px] font-medium text-foreground">
            {run.title}
          </span>
          {run.role && !told && (
            <span className="shrink-0 text-[11px] text-muted-stronger-foreground">
              {run.role}
            </span>
          )}
          {flagged && (
            <span
              className={cn(
                "shrink-0 text-[11px] font-medium",
                told ? "text-muted-foreground" : STATE_TEXT[run.state]
              )}
            >
              {told?.event ?? STATE_WORDS[run.state]}
            </span>
          )}
        </span>
        <span
          className={cn(
            "block truncate text-[12px]",
            run.state === "failed"
              ? "text-destructive"
              : "text-muted-foreground"
          )}
        >
          {line}
        </span>
      </span>
      <span className="shrink-0 text-[12px] text-muted-stronger-foreground tabular-nums">
        {time}
      </span>
      <ChevronRightIcon className="size-3 shrink-0 text-muted-stronger-foreground" />
    </button>
  )
}

/** Agents started one after the other: their avatars, how many and how they are doing, which opens into a card of them. */
function AgentGroup({
  agents,
  elapsed,
}: {
  agents: StartedAgent[]
  elapsed: (since: number) => string
}) {
  const [open, setOpen] = useState(true)
  const count = (state: AgentState) =>
    agents.filter((run) => run.state === state).length
  const order: AgentState[] = ["waiting", "working", "done", "failed"]
  const counted = order.filter((state) => count(state) > 0)
  const working = agents.filter((run) => run.since !== undefined)
  const longest = Math.max(...working.map((run) => run.since ?? 0))
  const took = agents
    .map((run) => run.took ?? "")
    .reduce((most, one) => (seconds(one) > seconds(most) ? one : most), "")
  const Chevron = open ? ChevronDownIcon : ChevronRightIcon
  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="-mx-1.5 flex h-[50px] w-[calc(100%+12px)] items-center gap-2.5 rounded-md px-1.5 text-left hover:bg-background-accent-larger"
      >
        <span className="flex shrink-0 -space-x-1.5">
          {agents.slice(0, 3).map((run, index) => (
            <AgentAvatar key={index} agent={run.agent} cut={index > 0} />
          ))}
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-[13px] font-semibold text-foreground">
            {agents.length} agents
          </span>
          <span className="block truncate text-[12px]">
            {counted.map((state, index) => (
              <Fragment key={state}>
                {index > 0 && (
                  <span className="text-muted-stronger-foreground"> · </span>
                )}
                <span className={STATE_TEXT[state]}>
                  {count(state)} {state}
                </span>
              </Fragment>
            ))}
          </span>
        </span>
        <span className="shrink-0 text-[12px] text-muted-stronger-foreground tabular-nums">
          {working.length > 0 ? elapsed(longest) : took}
        </span>
        <Chevron className="size-3 shrink-0 text-muted-stronger-foreground" />
      </button>
      {open && (
        <div className="mt-1 mb-2.5 rounded-lg border border-border-card bg-card p-1">
          {agents.map((run, index) => (
            <AgentRow key={index} run={run} elapsed={elapsed} card />
          ))}
        </div>
      )}
    </>
  )
}

/** "1m 04s" in seconds. */
function seconds(took: string) {
  const minutes = Number(/(\d+)m/.exec(took)?.[1] ?? 0)
  return minutes * 60 + Number(/(\d+)s/.exec(took)?.[1] ?? 0)
}

function Handoff({ end }: { end: HandoffEnd }) {
  return (
    <span className="flex items-center gap-1.5">
      <AgentIcon agent={end.agent} size={14} />
      <span className="text-foreground">{end.label}</span>
    </span>
  )
}

export function Shimmer({
  children,
  className,
}: {
  children: ReactNode
  className?: string
}) {
  return <span className={cn("demo-shimmer", className)}>{children}</span>
}

/** The bit of Markdown the replies use: paragraphs, headings, lists, `code` and **bold**. */
function Prose({ text }: { text: string }) {
  return (
    <div className="my-2 text-[14px] leading-6 text-muted-foreground">
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
    <ul className="mt-2 flex list-disc flex-col gap-1 pl-5 text-[14px] font-normal text-muted-foreground marker:text-muted-stronger-foreground">
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
          className="rounded-xs bg-background-accent px-1 py-px font-mono text-[12.5px] text-foreground"
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
    <div className="mt-3 mb-3 rounded-lg border bg-background-secondary pb-1.5">
      <div className="flex h-10 items-center justify-between pr-1.5 pl-3.5">
        <span className="text-[13px] font-medium text-foreground">
          {files.length} files changed{" "}
          <LineCounts added={added} removed={removed} />
        </span>
        <button
          type="button"
          onClick={onOpenDiff}
          className="h-7 rounded-md px-2 text-[12.5px] font-medium text-foreground hover:bg-background-secondary-accent"
        >
          Open diff
        </button>
      </div>
      {files.map((file) => (
        <button
          type="button"
          key={file.path}
          onClick={onOpenDiff}
          className="mx-1.5 flex h-7 w-[calc(100%-12px)] items-center gap-2 rounded-md px-1.5 text-left text-[12.5px] hover:bg-background-secondary-accent active:bg-background-secondary-accent"
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
