import {
  ChartColumnIcon,
  ChevronRightIcon,
  CircleDashedIcon,
  EyeIcon,
  GitBranchIcon,
  GitPullRequestIcon,
  RefreshCwIcon,
  SearchIcon,
  ServerIcon,
  SettingsIcon,
  ShieldIcon,
  type LucideIcon,
} from "lucide-react"
import { useState, type ReactNode } from "react"
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
  const matches = (title: string, project: string) =>
    `${title} ${project}`.toLowerCase().includes(search.toLowerCase())
  const active = threads.filter((thread) =>
    matches(thread.title, thread.project.name)
  )
  const done = doneThreads.filter((thread) =>
    matches(thread.title, thread.project.name)
  )

  return (
    <div className="flex h-full flex-col">
      <div className="mx-2.5 mt-0.5 mb-1.5 flex shrink-0 gap-2">
        <label className="flex h-7 min-w-0 flex-1 items-center gap-1.5 rounded-md bg-input px-[9px]">
          <SearchIcon className="size-3 text-muted-stronger-foreground" />
          <input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
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
        {active.map((thread) => (
          <ThreadRow
            key={thread.id}
            thread={thread}
            selected={thread.id === selectedId}
            elapsed={elapsed}
            onSelect={() => {
              onPoint(null)
              onSelect(thread.id)
            }}
            onPoint={onPoint}
          />
        ))}
        {active.length === 0 && (
          <p className="px-[18px] py-[7px] text-[12px] text-muted-stronger-foreground">
            No threads found
          </p>
        )}
      </div>
      {done.length > 0 && (
        <div className="border-t">
          <button
            type="button"
            onClick={() => setDoneOpen(!doneOpen)}
            className="flex h-[36px] w-full items-center gap-[7px] pr-[18px] pl-[11px] text-left text-muted-foreground hover:bg-background-accent-larger hover:text-foreground"
          >
            <ChevronRightIcon
              className={cn(
                "size-2.5 w-3.5 transition-transform",
                doneOpen && "rotate-90"
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
              {done.length}
            </span>
          </button>
          {doneOpen &&
            done.map((thread) => (
              <div
                key={thread.title}
                onPointerEnter={(event) => onPoint(thread, event.currentTarget)}
                onPointerLeave={() => onPoint(null)}
                className="mx-2.5 flex h-[30px] items-center gap-[7px] rounded-md px-2 hover:bg-background-accent-larger"
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
          className="flex h-[36px] w-full items-center gap-[7px] pr-[18px] pl-[11px] text-left text-muted-foreground hover:bg-background-accent-larger hover:text-foreground"
        >
          <ChevronRightIcon
            className={cn(
              "size-2.5 w-3.5 transition-transform",
              serversOpen && "rotate-90"
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
                <span className="flex w-3.5 justify-center">
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
      <div className="flex flex-col border-t px-[18px] py-2.5">
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

function FooterButton({ icon: Icon, label }: { icon: LucideIcon; label: string }) {
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
        selected ? "bg-background-accent-larger-stronger" : "hover:bg-background-accent-larger"
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
        <span className="text-[11px] text-muted-stronger-foreground">{thread.status.ago}</span>
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
