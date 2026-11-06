import {
  AlarmClockIcon,
  BotIcon,
  ChartColumnIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  CircleDashedIcon,
  CircleDotDashedIcon,
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
import {
  archivedThreads,
  servers,
  snoozedThreads,
  type ShelvedThread,
  type Thread,
} from "./threads"
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
  onPoint: (thread: Thread | ShelvedThread | null, row?: HTMLElement) => void
}) {
  const [search, setSearch] = useState("")
  const [workingOpen, setWorkingOpen] = useState(true)
  const [snoozedOpen, setSnoozedOpen] = useState(false)
  const [archivedOpen, setArchivedOpen] = useState(false)
  const [serversOpen, setServersOpen] = useState(false)
  const [found, setFound] = useState<Found[]>([])
  const [searching, setSearching] = useState(false)
  const [highlighted, setHighlighted] = useState(0)
  /** The shelved thread opened from the search, which its shelf shows. */
  const [revealed, setRevealed] = useState<string | null>(null)
  const query = normalized(search)
  const results = searchResults(threads, query, found)
  const active = threads.filter((thread) => !isWorking(thread))
  const working = threads.filter(isWorking)

  // What was found for the search before stays while it fits, until the messages are searched.
  const changeSearch = (text: string) => {
    setSearch(text)
    const next = normalized(text)
    if (next === query) return
    setHighlighted(0)
    setFound((found) =>
      searchable(next)
        ? found.filter((one) => hasWords(one.excerpt, next))
        : []
    )
    setSearching(searchable(next))
  }

  // As the server would: a moment after the search stops changing, the messages are searched.
  useEffect(() => {
    if (!searchable(query)) return
    const timer = setTimeout(() => {
      setFound(searchMessages(threads, query))
      setSearching(false)
    }, 450)
    return () => clearTimeout(timer)
  }, [query, threads])

  const open = (result: Result) => {
    onPoint(null)
    if (result.kind === "active") onSelect(result.thread.id)
    if (result.kind === "snoozed") setSnoozedOpen(true)
    if (result.kind === "archived") setArchivedOpen(true)
    setRevealed(result.kind === "active" ? null : result.thread.title)
    changeSearch("")
  }

  return (
    <div className="flex h-full flex-col">
      <div className="mx-2 mt-0.5 mb-1.5 flex shrink-0 gap-2">
        <label className="flex h-8 min-w-0 flex-1 items-center gap-1.5 rounded-md bg-input px-[9px]">
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
        className="flex min-h-0 flex-1 flex-col overflow-y-auto py-[3px]"
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
        {query && (results.length === 0 || searching) && (
          <p className="flex items-center gap-1.5 px-4 py-[7px] text-[12px] text-muted-stronger-foreground">
            {searching && <LoaderIcon className="size-3 animate-spin" />}
            {searching ? "Searching messages" : "No threads found"}
          </p>
        )}
        {!query &&
          active.map((thread) => (
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
        {!query && (working.length > 0 || snoozedThreads.length > 0 || archivedThreads.length > 0) && (
          <div className="mt-auto pt-px pb-0.5">
          {!query && working.length > 0 && (
            <Section
              title="Working"
              count={working.length}
              open={workingOpen}
              divided
              onToggle={() => setWorkingOpen(!workingOpen)}
            >
              {working.map((thread) => (
                <SlimRow
                  key={thread.id}
                  thread={thread}
                  lit={thread.id === selectedId}
                  onSelect={() => {
                    onPoint(null)
                    setRevealed(null)
                    onSelect(thread.id)
                  }}
                  onPoint={onPoint}
                >
                  <ThreadStatus thread={thread} elapsed={elapsed} />
                </SlimRow>
              ))}
            </Section>
          )}
          {!query && (
            <Section
              title="Snoozed"
              count={snoozedThreads.length}
              open={snoozedOpen}
              divided
              onToggle={() => setSnoozedOpen(!snoozedOpen)}
            >
              {snoozedThreads.map((thread) => (
                <SlimRow
                  key={thread.title}
                  thread={thread}
                  lit={thread.title === revealed}
                  onPoint={onPoint}
                >
                  <span className="flex items-center gap-[3px]">
                    <AlarmClockIcon className="size-3.5" />
                    {thread.ago}
                  </span>
                </SlimRow>
              ))}
            </Section>
          )}
          {!query && (
            <Section
              title="Archived"
              count={archivedThreads.length}
              open={archivedOpen}
              onToggle={() => setArchivedOpen(!archivedOpen)}
            >
              {archivedThreads.map((thread) => (
                <SlimRow
                  key={thread.title}
                  thread={thread}
                  lit={thread.title === revealed}
                  onPoint={onPoint}
                >
                  {thread.ago}
                </SlimRow>
              ))}
            </Section>
          )}
          </div>
        )}
      </div>
      <div className="border-t">
        <button
          type="button"
          onClick={() => setServersOpen(!serversOpen)}
          className="group flex h-[36px] w-full items-center gap-[7px] px-4 text-left text-muted-foreground hover:bg-background-accent-larger hover:text-foreground"
        >
          <StateRing />
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
          <Chevron open={serversOpen} />
        </button>
        {serversOpen && (
          <div className="pb-1">
            {servers.map((server) => (
              <div
                key={server.name}
                className="mx-2 flex h-8 items-center gap-[7px] px-2"
              >
                <StateRing />
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
            className="ml-[3px] flex size-8 items-center justify-center rounded-md hover:bg-background-accent"
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
  | { kind: "snoozed" | "archived"; thread: ShelvedThread }

const normalized = (search: string) => search.trim().split(/\s+/).join(" ")

const contains = (text: string, query: string) =>
  query !== "" && text.toLowerCase().includes(query.toLowerCase())

const words = (query: string) => query.toLowerCase().split(" ").filter(Boolean)

/** The servers search messages for a search with a word of three letters or more. */
const searchable = (query: string) => words(query).some((word) => word.length >= 3)

/** Every word of the search is in the text, in any order. */
const hasWords = (text: string, query: string) =>
  words(query).every((word) => text.toLowerCase().includes(word))

/** The threads found by their title, project or pull request, active before snoozed and archived, then the ones
 * found by a message. */
function searchResults(
  threads: Thread[],
  query: string,
  found: Found[]
): Result[] {
  if (!query) return []
  const named = (thread: Thread | ShelvedThread) =>
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
    ...snoozedThreads
      .filter(named)
      .map((thread) => ({ kind: "snoozed" as const, thread })),
    ...archivedThreads
      .filter(named)
      .map((thread) => ({ kind: "archived" as const, thread })),
    ...byMessage,
  ]
}

/** Each thread's best message with the search in it: the user's before the agent's, then the latest. */
function searchMessages(threads: Thread[], query: string): Found[] {
  return threads.flatMap((thread) => {
    const messages = thread.items.flatMap((item) =>
      (item.kind === "user" || item.kind === "text") &&
      hasWords(item.text, query)
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
  const places = words(query).map((word) => line.toLowerCase().indexOf(word))
  const found = Math.max(0, Math.min(...places))
  let start = Math.max(0, found - 24)
  if (start > 0) {
    const space = line.slice(start, found).indexOf(" ")
    if (space >= 0) start += space + 1
  }
  return (start > 0 ? "…" : "") + line.slice(start)
}

/** The excerpt with each word of the search set off. */
function Excerpt({ text, query }: { text: string; query: string }) {
  const long = words(query).filter((word) => word.length >= 3)
  const escaped = long.map((word) => word.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
  const pieces = text.split(new RegExp(`(${escaped.join("|")})`, "i"))
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
  const shelved = result.kind !== "active"
  const found = result.kind === "active" ? result.found : undefined
  return (
    <button
      type="button"
      onClick={onOpen}
      onPointerEnter={(event) => onPoint(event.currentTarget)}
      onPointerLeave={() => onPoint()}
      className={cn(
        "mx-2 my-px flex w-[calc(100%-16px)] flex-col rounded-md px-2 pt-[5px] pb-[7px] text-left",
        highlighted && "bg-background-accent-larger"
      )}
    >
      <span className="flex h-[22px] w-full items-center gap-1.5">
        <span
          className={cn(shelved && !highlighted && "opacity-(--opacity-disabled)")}
        >
          <ProjectIcon project={thread.project} size={14} />
        </span>
        <span
          className={cn(
            "truncate text-[13px] font-medium",
            highlighted
              ? "text-foreground"
              : shelved
                ? "text-muted-stronger-foreground"
                : "text-muted-foreground"
          )}
        >
          {thread.title}
        </span>
        <span className="ml-auto flex shrink-0 items-center gap-1.5">
          {thread.pullRequest && (
            <PullRequest number={thread.pullRequest.number} quiet={shelved} />
          )}
          {result.kind === "active" ? (
            <ThreadStatus thread={result.thread} elapsed={elapsed} />
          ) : (
            <span className="text-[11px] text-muted-stronger-foreground">
              {result.thread.ago}
            </span>
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

/** A thread works there, or an agent it started does, and nothing waits for the user. */
const isWorking = (thread: Thread) =>
  thread.status.kind === "working" ||
  thread.status.kind === "monitoring" ||
  thread.status.kind === "agents"

/** A part of the list at its bottom: its line, laid out as a row, with its name lit only under the pointer, how
 * many threads it holds, always the quietest, and its chevron, then its rows, with a line under them when
 * another part follows. */
function Section({
  title,
  count,
  open,
  divided = false,
  onToggle,
  children,
}: {
  title: string
  count: number
  open: boolean
  divided?: boolean
  onToggle: () => void
  children: ReactNode
}) {
  return (
    <>
      <button
        type="button"
        onClick={onToggle}
        className="group mx-2 my-px flex h-8 w-[calc(100%-16px)] items-center gap-[7px] rounded-md px-2 text-left text-muted-foreground hover:bg-background-accent-larger hover:text-foreground"
      >
        <span className="text-[12px] font-medium">{title}</span>
        <span className="text-[11px] text-muted-strongest-foreground tabular-nums">
          {count}
        </span>
        <span className="ml-auto" />
        <Chevron open={open} />
      </button>
      {open && children}
      {open && divided && <div className="mx-4 my-[5px] h-px bg-border" />}
    </>
  )
}

function Chevron({ open }: { open: boolean }) {
  const Icon = open ? ChevronDownIcon : ChevronRightIcon
  return (
    <Icon className="-mr-[3px] size-3 text-muted-stronger-foreground group-hover:text-muted-foreground" />
  )
}

/** A thread in one of those parts: one quiet line, with what it is up to at its end, lit under the pointer. */
function SlimRow({
  thread,
  lit,
  onSelect,
  onPoint,
  children,
}: {
  thread: Thread | ShelvedThread
  lit: boolean
  onSelect?: () => void
  onPoint: (thread: Thread | ShelvedThread | null, row?: HTMLElement) => void
  children: ReactNode
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      onPointerEnter={(event) => onPoint(thread, event.currentTarget)}
      onPointerLeave={() => onPoint(null)}
      className={cn(
        "group mx-2 my-px flex h-8 w-[calc(100%-16px)] items-center gap-[7px] rounded-md px-2 text-left",
        lit
          ? "bg-background-accent-larger-stronger"
          : "hover:bg-background-accent-larger"
      )}
    >
      <ProjectIcon project={thread.project} size={16} />
      <span
        className={cn(
          "truncate text-[13px] group-hover:text-foreground",
          lit ? "text-foreground" : "text-muted-foreground"
        )}
      >
        {thread.title}
      </span>
      <span className="ml-auto flex shrink-0 items-center gap-1.5 text-[11px] text-muted-stronger-foreground">
        {thread.pullRequest && (
          <PullRequest number={thread.pullRequest.number} quiet />
        )}
        {children}
      </span>
    </button>
  )
}

/** A server's state: a dot in a ring of its colour's tint, as wide as a shelf's symbol. */
function StateRing() {
  return (
    <span className="flex w-4 shrink-0 justify-center">
      <span className="flex size-3 items-center justify-center rounded-full border border-success/(--opacity-color-tint-chart-stronger)">
        <span className="size-1.5 rounded-full bg-success" />
      </span>
    </span>
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
      className="flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-background-accent hover:text-foreground"
    >
      <Icon className="size-4" />
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
        "group mx-2 my-px flex w-[calc(100%-16px)] flex-col rounded-md px-2 pt-[5px] pb-[7px] text-left",
        selected
          ? "bg-background-accent-larger-stronger"
          : "hover:bg-background-accent-larger"
      )}
    >
      <span className="flex h-5 w-full items-center gap-1.5 text-muted-foreground">
        <ProjectIcon project={thread.project} size={14} />
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
          <GitBranchIcon className="size-3.5 shrink-0" />
          <span className="truncate">{thread.branch}</span>
        </span>
        <span className="ml-auto flex items-center gap-[9px]">
          {thread.pullRequest && (
            <PullRequest number={thread.pullRequest.number} />
          )}
          <span className="flex items-center gap-[3px] text-[11px]">
            <ServerIcon className="size-3.5" />
            {thread.server.name}
          </span>
          <AgentIcon agent={thread.agent} size={14} />
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
          <ShieldIcon className="size-3.5" />
          Approval
        </span>
      )
    case "working":
      return (
        <span className={cn(label, "text-process")}>
          <CircleDashedIcon className="size-3.5" />
          {elapsed(thread.status.since)}
        </span>
      )
    case "agents":
      return (
        <span className="flex items-center gap-[9px]">
          <span className={cn(label, "text-pending")}>
            <BotIcon className="size-3.5" />
            {thread.status.count}
          </span>
          <span className={cn(label, "text-process")}>
            <CircleDotDashedIcon className="size-3.5" />
            {elapsed(thread.status.since)}
          </span>
        </span>
      )
    case "monitoring":
      return (
        <span className={cn(label, "text-foreground")}>
          <EyeIcon className="size-3.5" />
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
        "flex cursor-pointer items-center gap-[3px] text-[11px] font-medium tabular-nums hover:underline",
        quiet ? "text-muted-stronger-foreground" : "text-success"
      )}
    >
      <GitPullRequestIcon className="size-3.5" />
      {number}
    </span>
  )
}
