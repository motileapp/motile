import {
  FolderGit2Icon,
  GitBranchIcon,
  GitPullRequestIcon,
  ServerIcon,
} from "lucide-react"
import { useRef, useState, type ReactNode, type RefObject } from "react"
import { Dotted } from "./composer"
import { AgentIcon, ProjectIcon } from "./icons"
import type { DoneThread, Thread } from "./threads"
import { cn } from "@/lib/utils"

type CardThread = Pick<
  Thread,
  | "title"
  | "project"
  | "branch"
  | "agent"
  | "model"
  | "server"
  | "pullRequest"
  | "worktree"
>

type Peek = { thread: CardThread; top: number; left: number; fading: boolean }

/** How long the pointer rests on a row before its card comes up, and how long after the card
 * went away the next one still comes up at once. */
const REST = 300
const LINGER = 400
/** Between the row's light and the card. */
const GAP = 4

/** The card of the thread whose row the pointer rests on, as the Mac app shows it beside the
 * row, in the window the demo draws. */
export function useThreadPeek(demoWindow: RefObject<HTMLDivElement | null>) {
  const [peek, setPeek] = useState<Peek | null>(null)
  const shown = useRef(false)
  const hiddenAt = useRef(0)
  const waiting = useRef<number | undefined>(undefined)

  const point = (thread: CardThread | DoneThread | null, row?: HTMLElement) => {
    clearTimeout(waiting.current)
    const frame = demoWindow.current
    if (!thread || !row || !frame) {
      if (shown.current) hiddenAt.current = Date.now()
      shown.current = false
      setPeek(null)
      return
    }
    const place = (fading: boolean) => {
      const outer = frame.getBoundingClientRect()
      const scale = outer.width / frame.offsetWidth
      const light = row.getBoundingClientRect()
      const top = (light.top - outer.top) / scale
      const left = (light.right - outer.left) / scale + GAP
      shown.current = true
      setPeek({ thread, top, left, fading })
    }
    if (shown.current || Date.now() - hiddenAt.current < LINGER) {
      place(false)
      return
    }
    waiting.current = window.setTimeout(() => place(true), REST)
  }

  return { peek, point }
}

/** The card in a layer as tall as the window, level with the row's top, lifted where it would
 * pass the window's bottom. */
export function ThreadCardLayer({ peek }: { peek: Peek | null }) {
  if (!peek) return null
  return (
    <div
      className="pointer-events-none absolute top-0 bottom-2 z-30 flex flex-col items-start"
      style={{ left: peek.left }}
    >
      <div className="min-h-0 shrink" style={{ height: peek.top }} />
      <ThreadCard
        thread={peek.thread}
        className={cn(peek.fading && "animate-in duration-100 fade-in-0")}
      />
    </div>
  )
}

function ThreadCard({
  thread,
  className,
}: {
  thread: CardThread
  className?: string
}) {
  const Checkout = thread.worktree ? FolderGit2Icon : GitBranchIcon
  return (
    <div
      className={cn(
        "max-w-[320px] shrink-0 rounded-md border border-border-popover bg-popover p-3 shadow-md",
        className
      )}
    >
      <p className="truncate text-[13px] font-medium text-foreground">
        {thread.title}
      </p>
      <div className="flex flex-col gap-2.5 pt-2.5">
        <Line icon={<ProjectIcon project={thread.project} />}>
          {thread.project.name}
        </Line>
        <Line icon={<Checkout className="size-[13px]" />}>{thread.branch}</Line>
        <Line icon={<AgentIcon agent={thread.agent} size={13} />}>
          <Dotted text={thread.model} />
        </Line>
        <Line icon={<ServerIcon className="size-3" />}>
          {thread.server.name}
        </Line>
      </div>
      {thread.pullRequest && (
        <>
          <div className="my-2.5 h-px bg-border-popover" />
          <p className="flex items-center gap-2 text-[12px]">
            <span className="flex w-3.5 justify-center text-success">
              <GitPullRequestIcon className="size-[13px]" />
            </span>
            <span className="font-medium text-muted-stronger-foreground tabular-nums">
              #{thread.pullRequest.number}
            </span>
            <span className="min-w-0 truncate text-muted-foreground">
              {thread.pullRequest.title}
            </span>
          </p>
        </>
      )}
    </div>
  )
}

function Line({ icon, children }: { icon: ReactNode; children: ReactNode }) {
  return (
    <p className="flex items-center gap-2 text-[12px] text-muted-foreground">
      <span className="flex size-3.5 shrink-0 items-center justify-center">
        {icon}
      </span>
      <span className="min-w-0 truncate">{children}</span>
    </p>
  )
}
