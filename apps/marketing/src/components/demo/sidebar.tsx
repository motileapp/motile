import {
  ChevronRightIcon,
  CircleDashedIcon,
  CircleQuestionMarkIcon,
  CircleUserIcon,
  EyeIcon,
  GitPullRequestIcon,
  SearchIcon,
  ServerIcon,
} from "lucide-react"
import { useState } from "react"
import { AgentIcon, ProjectIcon } from "./icons"
import { doneThreads, servers, type Thread } from "./threads"
import { cn } from "@/lib/utils"

export function Sidebar({
  threads,
  selectedId,
  elapsed,
  onSelect,
}: {
  threads: Thread[]
  selectedId: string
  elapsed: (since: number) => string
  onSelect: (id: string) => void
}) {
  const [search, setSearch] = useState("")
  const [doneOpen, setDoneOpen] = useState(false)
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
      <label className="mx-2.5 mt-0.5 mb-1.5 flex h-7 shrink-0 items-center gap-1.5 rounded-[7px] bg-background-secondary px-[9px]">
        <SearchIcon className="size-3 text-tertiary" />
        <input
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          placeholder="Search"
          className="w-full bg-transparent text-[12.5px] outline-none placeholder:text-tertiary pointer-coarse:text-[16px]"
        />
      </label>
      <div className="min-h-0 flex-1 overflow-y-auto py-[3px]">
        {active.map((thread) => (
          <ThreadRow
            key={thread.id}
            thread={thread}
            selected={thread.id === selectedId}
            elapsed={elapsed}
            onSelect={() => onSelect(thread.id)}
          />
        ))}
        {active.length === 0 && (
          <p className="px-[18px] py-[7px] text-[12px] text-tertiary">
            No threads found
          </p>
        )}
      </div>
      {done.length > 0 && (
        <div className="border-t">
          <button
            type="button"
            onClick={() => setDoneOpen(!doneOpen)}
            className="flex h-[38px] w-full items-center gap-[7px] px-[18px] text-left hover:bg-background-secondary"
          >
            <ChevronRightIcon
              className={cn(
                "size-2.5 w-3.5 transition-transform",
                doneOpen && "rotate-90"
              )}
            />
            <span className="text-[12px] font-medium">Done</span>
            <span className="ml-auto text-[11px] text-tertiary tabular-nums">
              {done.length}
            </span>
          </button>
          {doneOpen &&
            done.map((thread) => (
              <div
                key={thread.title}
                className="mx-2.5 flex h-[30px] items-center gap-[7px] rounded-lg px-2 hover:bg-background-secondary"
              >
                <ProjectIcon project={thread.project} />
                <span className="truncate text-[13px] text-muted-foreground">
                  {thread.title}
                </span>
                <span className="ml-auto flex shrink-0 items-center gap-1.5 text-[11px] text-tertiary">
                  {thread.pullRequest && (
                    <PullRequest number={thread.pullRequest} quiet />
                  )}
                  {thread.ago}
                </span>
              </div>
            ))}
          {doneOpen && <div className="h-1" />}
        </div>
      )}
      <div className="flex flex-col gap-2 border-t px-[18px] pt-2.5 pb-2">
        {servers.map((server) => (
          <div key={server.name} className="flex h-5 items-center gap-[7px]">
            <span className="size-[7px] rounded-full bg-success" />
            <span className="text-[12px] font-medium">{server.name}</span>
            <span className="ml-auto text-[11px] text-tertiary tabular-nums">
              {server.path} · {server.ms} ms
            </span>
          </div>
        ))}
        <div className="-mx-2 flex h-[30px] items-center gap-[7px] rounded-lg px-2 hover:bg-background-secondary">
          <CircleUserIcon className="size-3.5" />
          <span className="text-[12px]">you@motile.app</span>
        </div>
      </div>
    </div>
  )
}

function ThreadRow({
  thread,
  selected,
  elapsed,
  onSelect,
}: {
  thread: Thread
  selected: boolean
  elapsed: (since: number) => string
  onSelect: () => void
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className={cn(
        "mx-2.5 my-px flex w-[calc(100%-20px)] flex-col rounded-lg px-2 pt-[3px] pb-[7px] text-left",
        selected ? "bg-background-tertiary" : "hover:bg-background-secondary"
      )}
    >
      <span className="flex h-[22px] w-full items-center gap-1.5 pb-0.5 text-muted-foreground">
        <ProjectIcon project={thread.project} />
        <span className="text-[11px] font-medium">{thread.project.name}</span>
        <span className="ml-auto">
          <ThreadStatus thread={thread} elapsed={elapsed} />
        </span>
      </span>
      <span className="w-full truncate pb-1 text-[13px] font-medium">
        {thread.title}
      </span>
      <span className="flex h-4 w-full items-center gap-1.5 text-tertiary">
        <span className="truncate text-[11px]">{thread.branch}</span>
        <span className="ml-auto flex items-center gap-1.5">
          {thread.pullRequest && <PullRequest number={thread.pullRequest} />}
          <span className="flex items-center gap-[3px] text-[11px]">
            <ServerIcon className="size-[9px]" />
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
          <CircleQuestionMarkIcon className="size-[11px]" />
          Approval
        </span>
      )
    case "working":
      return (
        <span className={cn(label, "text-working")}>
          <CircleDashedIcon className="size-[11px]" />
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
        <span className="text-[11px] text-tertiary">{thread.status.ago}</span>
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
        "flex items-center gap-0.5 text-[11px] font-medium tabular-nums",
        quiet ? "text-tertiary" : "text-success"
      )}
    >
      <GitPullRequestIcon className="size-[11px]" />
      {number}
    </span>
  )
}
