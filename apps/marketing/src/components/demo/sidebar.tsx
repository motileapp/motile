import {
  ChartColumnIcon,
  ChevronRightIcon,
  CircleDashedIcon,
  EyeIcon,
  GitBranchIcon,
  GitPullRequestIcon,
  LoaderIcon,
  RefreshCwIcon,
  SearchIcon,
  ServerIcon,
  SettingsIcon,
  ShieldIcon,
  type LucideIcon,
} from "lucide-react"
import { useEffect, useState, type ReactNode } from "react"
import { AgentIcon, ProjectIcon } from "./icons"
import { doneThreads, servers, type DoneThread, type Thread } from "./threads"
import { cn } from "@/lib/utils"

const serversPaths = new Set(servers.map((server) => server.path))
const serversReach = `${serversPaths.size > 1 ? "Mixed" : servers[0].path} · ${Math.max(...servers.map((server) => server.ms))} ms`

export function Sidebar({
  actions,
  threads,
  selectedId,
  elapsed,
  onSelect,
  onPoint,
}: {
  actions: ReactNode
  threads: Thread[]
  selectedId: string
  elapsed: (since: number) => string
  onSelect: (id: string) => void
  /** The pointer is on a thread's row, or on none. */
  onPoint: (thread: Thread | DoneThread | null, row?: HTMLElement) => void
}) {
  const [search, setSearch] = useState("")
  const [doneOpen, setDoneOpen] = useState(false)
  const [serversOpen, setServersOpen] = useState(false)
  const [found, setFound] = useState<Found[]>([])
  const [searching, setSearching] = useState(false)
  const [highlighted, setHighlighted] = useState(0)
  /** The done thread opened from the search, which the Done shelf shows. */
  const [revealed, setRevealed] = useState<string | null>(null)
  const query = normalized(search)
  const results = searchResults(threads, query, found)

  // What was found for the search before stays while it fits, until the messages are searched.
  const changeSearch = (text: string) => {
    setSearch(text)
    const next = normalized(text)
    if (next === query) return
    setHighlighted(0)
    setFound((found) =>
      next.length < 2 ? [] : found.filter((one) => contains(one.excerpt, next))
    )
    setSearching(next.length >= 2)
  }

  // As the server would: a moment after the search stops changing, the messages are searched.
  useEffect(() => {
    if (query.length < 2) return
    const timer = setTimeout(() => {
      setFound(searchMessages(threads, query))
      setSearching(false)
    }, 450)
    return () => clearTimeout(timer)
  }, [query, threads])

  const open = (result: Result) => {
    onPoint(null)
    if (result.kind === "active") onSelect(result.thread.id)
    if (result.kind === "done") setDoneOpen(true)
    setRevealed(result.kind === "done" ? result.thread.title : null)
    changeSearch("")
  }

  return (
    <div className="flex h-full flex-col">
      <div className="mx-2.5 mt-0.5 mb-1.5 flex shrink-0 gap-2">
        <label className="flex h-7 min-w-0 flex-1 items-center gap-1.5 rounded-md bg-input px-[9px]">
          <SearchIcon className="size-3 text-muted-stronger-foreground" />
          <input
            value={search}
            onChange={(event) => changeSearch(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") changeSearch("")
              if (results.length === 0) return
              const step = { ArrowDown: 1, ArrowUp: -1 }[event.key]
              if (step) {
                event.preventDefault()
                setHighlighted(
                  (highlighted + step + results.length) % results.length
                )
              }
              if (event.key === "Enter") open(results[highlighted])
            }}
            placeholder="Search"
            className="w-full bg-transparent text-[12.5px] outline-none placeholder:text-muted-stronger-foreground pointer-coarse:text-[16px]"
          />
        </label>
        <div className="flex">{actions}</div>
      </div>
      <div
        className="min-h-0 flex-1 overflow-y-auto py-[3px]"
        onScroll={() => onPoint(null)}
      >
        {query &&
          results.map((result, index) => (
            <ResultRow
              key={result.thread.title}
              result={result}
              query={query}
              highlighted={index === highlighted}
              elapsed={elapsed}
              onOpen={() => open(result)}
              onPoint={(row) => {
                setHighlighted(index)
                onPoint(row ? result.thread : null, row)
              }}
            />
          ))}
        {query && results.length === 0 && (
          <p className="flex items-center gap-1.5 px-[18px] py-[7px] text-[12px] text-muted-stronger-foreground">
            {searching && <LoaderIcon className="size-3 animate-spin" />}
            {searching ? "Searching messages" : "No threads found"}
          </p>
        )}
        {!query &&
          threads.map((thread) => (
            <ThreadRow
              key={thread.id}
              thread={thread}
              selected={thread.id === selectedId}
              elapsed={elapsed}
              onSelect={() => {
                onPoint(null)
                setRevealed(null)
                onSelect(thread.id)
              }}
              onPoint={onPoint}
            />
          ))}
      </div>
      {!query && (
        <div className="border-t">
          <button
            type="button"
            onClick={() => setDoneOpen(!doneOpen)}
            className="group flex h-[36px] w-full items-center gap-[7px] pr-[18px] pl-[11px] text-left text-muted-foreground hover:bg-background-accent-larger hover:text-foreground"
          >
            <ChevronRightIcon
              className={cn(
                "size-2.5 w-[9px] text-muted-stronger-foreground transition-transform group-hover:text-muted-foreground",
                doneOpen && "rotate-90 text-muted-foreground"
              )}
            />
            <span
              className={cn(
                "text-[12px] font-medium",
                doneOpen && "text-foreground"
              )}
            >
              Done
            </span>
            <span className="ml-auto text-[11px] text-muted-stronger-foreground tabular-nums">
              {doneThreads.length}
            </span>
          </button>
          {doneOpen &&
            doneThreads.map((thread) => (
              <div
                key={thread.title}
                onPointerEnter={(event) => onPoint(thread, event.currentTarget)}
                onPointerLeave={() => onPoint(null)}
                className={cn(
                  "mx-2.5 flex h-[30px] items-center gap-[7px] rounded-md px-2",
                  thread.title === revealed
                    ? "bg-background-accent-larger-stronger"
                    : "hover:bg-background-accent-larger"
                )}
              >
                <ProjectIcon project={thread.project} />
                <span className="truncate text-[13px] text-muted-foreground">
                  {thread.title}
                </span>
                <span className="ml-auto flex shrink-0 items-center gap-1.5 text-[11px] text-muted-stronger-foreground">
                  {thread.pullRequest && (
                    <PullRequest number={thread.pullRequest.number} quiet />
                  )}
                  {thread.ago}
                </span>
              </div>
            ))}
          {doneOpen && <div className="h-1" />}
        </div>
      )}
      <div className="border-t">
        <button
          type="button"
          onClick={() => setServersOpen(!serversOpen)}
          className="group flex h-[36px] w-full items-center gap-[7px] pr-[18px] pl-[11px] text-left text-muted-foreground hover:bg-background-accent-larger hover:text-foreground"
        >
          <ChevronRightIcon
            className={cn(
              "size-2.5 w-[9px] text-muted-stronger-foreground transition-transform group-hover:text-muted-foreground",
              serversOpen && "rotate-90 text-muted-foreground"
            )}
          />
          <span className="size-[7px] rounded-full bg-success" />
          <span
            className={cn(
              "text-[12px] font-medium",
              serversOpen && "text-foreground"
            )}
          >
            Servers
          </span>
          <span className="ml-auto text-[11px] text-muted-stronger-foreground tabular-nums">
            {serversReach}
          </span>
        </button>
        {serversOpen && (
          <div className="pb-1">
            {servers.map((server) => (
              <div
                key={server.name}
                className="mx-2.5 flex h-[30px] items-center gap-[7px] px-2"
              >
                <span className="flex w-[9px] justify-center">
                  <span className="size-[7px] rounded-full bg-success" />
                </span>
                <span className="text-[12px] font-medium">{server.name}</span>
                <span className="ml-auto text-[11px] text-muted-stronger-foreground tabular-nums">
                  {server.path} · {server.ms} ms
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
      <div className="flex flex-col border-t px-[18px] py-[9px]">
        <div className="-mx-[7px] flex items-center gap-1">
          <button
            type="button"
            aria-label="you@motile.app"
            title="you@motile.app"
            className="ml-[3px] flex size-7 items-center justify-center rounded-md hover:bg-background-accent"
          >
            <span className="flex size-5 items-center justify-center rounded-full bg-background-accent text-[10px] font-semibold text-foreground">
              Y
            </span>
          </button>
          <FooterButton icon={SettingsIcon} label="Settings" />
          <FooterButton icon={ChartColumnIcon} label="Usage" />
          <span className="ml-auto" />
          <FooterButton icon={RefreshCwIcon} label="Check for updates" />
        </div>
      </div>
    </div>
  )
}

/** A message the search found a thread by, cut a little before what was searched for. */
type Found = { threadId: string; byUser: boolean; excerpt: string }

type Result =
  | { kind: "active"; thread: Thread; found?: Found }
  | { kind: "done"; thread: DoneThread }

const normalized = (search: string) => search.trim().split(/\s+/).join(" ")

const contains = (text: string, query: string) =>
  query !== "" && text.toLowerCase().includes(query.toLowerCase())

/** The threads found by their title, project or pull request, active before done, then the ones
 * found by a message. */
function searchResults(
  threads: Thread[],
  query: string,
  found: Found[]
): Result[] {
  if (!query) return []
  const named = (thread: Thread | DoneThread) =>
    [
      thread.title,
      thread.project.name,
      thread.pullRequest && `#${thread.pullRequest.number}`,
      thread.pullRequest?.title,
    ].some((name) => name && contains(name, query))
  const byMessage = threads.flatMap((thread) => {
    const message = found.find((one) => one.threadId === thread.id)
    return !named(thread) && message
      ? [{ kind: "active" as const, thread, found: message }]
      : []
  })
  return [
    ...threads
      .filter(named)
      .map((thread) => ({ kind: "active" as const, thread })),
    ...doneThreads
      .filter(named)
      .map((thread) => ({ kind: "done" as const, thread })),
    ...byMessage,
  ]
}

/** Each thread's best message with the search in it: the user's before the agent's, then the latest. */
function searchMessages(threads: Thread[], query: string): Found[] {
  return threads.flatMap((thread) => {
    const messages = thread.items.flatMap((item) =>
      (item.kind === "user" || item.kind === "text") &&
      contains(item.text, query)
        ? [{ byUser: item.kind === "user", text: item.text }]
        : []
    )
    const best =
      messages.findLast((message) => message.byUser) ?? messages.at(-1)
    if (!best) return []
    return [
      {
        threadId: thread.id,
        byUser: best.byUser,
        excerpt: excerpt(best.text, query),
      },
    ]
  })
}

function excerpt(text: string, query: string) {
  const line = text.split(/\s+/).join(" ").trim()
  const found = Math.max(0, line.toLowerCase().indexOf(query.toLowerCase()))
  let start = Math.max(0, found - 24)
  if (start > 0) {
    const space = line.slice(start, found).indexOf(" ")
    if (space >= 0) start += space + 1
  }
  return (start > 0 ? "…" : "") + line.slice(start)
}

/** The excerpt with each match set off. */
function Excerpt({ text, query }: { text: string; query: string }) {
  const pieces = text.split(
    new RegExp(`(${query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`, "i")
  )
  return pieces.map((piece, index) =>
    index % 2 === 1 ? (
      <span key={index} className="font-semibold text-foreground">
        {piece}
      </span>
    ) : (
      piece
    )
  )
}

/** What the search found, on two lines as tall whatever was found. */
function ResultRow({
  result,
  query,
  highlighted,
  elapsed,
  onOpen,
  onPoint,
}: {
  result: Result
  query: string
  highlighted: boolean
  elapsed: (since: number) => string
  onOpen: () => void
  onPoint: (row?: HTMLElement) => void
}) {
  const { thread } = result
  const done = result.kind === "done"
  const found = result.kind === "active" ? result.found : undefined
  return (
    <button
      type="button"
      onClick={onOpen}
      onPointerEnter={(event) => onPoint(event.currentTarget)}
      onPointerLeave={() => onPoint()}
      className={cn(
        "mx-2.5 my-px flex w-[calc(100%-20px)] flex-col rounded-md px-2 pt-[5px] pb-[7px] text-left",
        highlighted && "bg-background-accent-larger"
      )}
    >
      <span className="flex h-[22px] w-full items-center gap-1.5">
        <span
          className={cn(done && !highlighted && "opacity-(--opacity-disabled)")}
        >
          <ProjectIcon project={thread.project} />
        </span>
        <span
          className={cn(
            "truncate text-[13px] font-medium",
            highlighted
              ? "text-foreground"
              : done
                ? "text-muted-stronger-foreground"
                : "text-muted-foreground"
          )}
        >
          {thread.title}
        </span>
        <span className="ml-auto flex shrink-0 items-center gap-1.5">
          {thread.pullRequest && (
            <PullRequest number={thread.pullRequest.number} quiet={done} />
          )}
          {result.kind === "done" ? (
            <span className="text-[11px] text-muted-stronger-foreground">
              {result.thread.ago}
            </span>
          ) : (
            <ThreadStatus thread={result.thread} elapsed={elapsed} />
          )}
        </span>
      </span>
      <span className="h-[18px] w-full truncate text-[12px] leading-[18px] text-muted-stronger-foreground">
        {found ? (
          <>
            <span className="font-medium text-muted-foreground">
              {found.byUser ? "You: " : "Agent: "}
            </span>
            <Excerpt text={found.excerpt} query={query} />
          </>
        ) : (
          thread.project.name
        )}
      </span>
    </button>
  )
}

function FooterButton({
  icon: Icon,
  label,
}: {
  icon: LucideIcon
  label: string
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className="flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-background-accent hover:text-foreground"
    >
      <Icon className="size-3.5" />
    </button>
  )
}

function ThreadRow({
  thread,
  selected,
  elapsed,
  onSelect,
  onPoint,
}: {
  thread: Thread
  selected: boolean
  elapsed: (since: number) => string
  onSelect: () => void
  onPoint: (thread: Thread | null, row?: HTMLElement) => void
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      onPointerEnter={(event) => onPoint(thread, event.currentTarget)}
      onPointerLeave={() => onPoint(null)}
      className={cn(
        "group mx-2.5 my-px flex w-[calc(100%-20px)] flex-col rounded-md px-2 pt-[5px] pb-[7px] text-left",
        selected
          ? "bg-background-accent-larger-stronger"
          : "hover:bg-background-accent-larger"
      )}
    >
      <span className="flex h-5 w-full items-center gap-1.5 text-muted-foreground">
        <ProjectIcon project={thread.project} />
        <span className="text-[11px] font-medium">{thread.project.name}</span>
        <span className="ml-auto">
          <ThreadStatus thread={thread} elapsed={elapsed} />
        </span>
      </span>
      <span
        className={cn(
          "w-full truncate pt-px pb-[5px] text-[13px] font-medium group-hover:text-foreground",
          selected ? "text-foreground" : "text-muted-foreground"
        )}
      >
        {thread.title}
      </span>
      <span className="flex h-4 w-full items-center gap-1.5 text-muted-stronger-foreground">
        <span className="flex min-w-0 items-center gap-[3px] text-[11px]">
          <GitBranchIcon className="size-[11px] shrink-0" />
          <span className="truncate">{thread.branch}</span>
        </span>
        <span className="ml-auto flex items-center gap-1.5">
          {thread.pullRequest && (
            <PullRequest number={thread.pullRequest.number} />
          )}
          <span className="ml-px flex items-center gap-[3px] text-[11px]">
            <ServerIcon className="size-[10px]" />
            {thread.server.name}
          </span>
          <AgentIcon agent={thread.agent} />
        </span>
      </span>
    </button>
  )
}

function ThreadStatus({
  thread,
  elapsed,
}: {
  thread: Thread
  elapsed: (since: number) => string
}) {
  const label =
    "flex items-center gap-[3px] text-[11px] font-medium tabular-nums"
  switch (thread.status.kind) {
    case "approval":
      return (
        <span className={cn(label, "text-warning")}>
          <ShieldIcon className="size-[11px]" />
          Approval
        </span>
      )
    case "working":
      return (
        <span className={cn(label, "text-process")}>
          <CircleDashedIcon className="size-[10px]" />
          {elapsed(thread.status.since)}
        </span>
      )
    case "monitoring":
      return (
        <span className={cn(label, "text-foreground")}>
          <EyeIcon className="size-[11px]" />
          {elapsed(thread.status.since)}
        </span>
      )
    case "idle":
      return (
        <span className="text-[11px] text-muted-stronger-foreground">
          {thread.status.ago}
        </span>
      )
  }
}

function PullRequest({
  number,
  quiet = false,
}: {
  number: number
  quiet?: boolean
}) {
  return (
    <span
      className={cn(
        "flex cursor-pointer items-center gap-0.5 text-[11px] font-medium tabular-nums hover:underline",
        quiet ? "text-muted-stronger-foreground" : "text-success"
      )}
    >
      <GitPullRequestIcon className="size-[11px]" />
      {number}
    </span>
  )
}
