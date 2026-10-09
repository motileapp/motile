import {
  ArrowUpIcon,
  FolderGit2Icon,
  FolderIcon,
  GitBranchIcon,
  LockOpenIcon,
  PaperclipIcon,
  ServerIcon,
  ShieldIcon,
  SquareTerminalIcon,
} from "lucide-react"
import { useRef, useState, type ReactNode } from "react"
import { AgentIcon, MenuChevron, ProjectIcon } from "./icons"
import type { Thread } from "./threads"
import { cn } from "@/lib/utils"

export function Composer({
  thread,
  elapsed,
  onSend,
  onAnswer,
  onStop,
}: {
  thread: Thread
  elapsed: (since: number) => string
  onSend: (text: string) => void
  onAnswer: (allow: boolean) => void
  onStop: () => void
}) {
  const [text, setText] = useState("")
  const input = useRef<HTMLTextAreaElement>(null)
  const status = thread.status
  const send = () => {
    if (!text.trim()) return
    onSend(text.trim())
    setText("")
  }

  return (
    <div className="mx-auto w-full max-w-[796px]">
      {status.kind === "monitoring" && (
        <Strip edge="top">
          <span className="mr-2 ml-3.5 size-1.5 rounded-full bg-foreground" />
          <span className="text-[12.5px] font-medium tabular-nums text-foreground">
            Monitoring for {elapsed(status.since)}
          </span>
          <button
            type="button"
            onClick={onStop}
            className="mr-1 ml-auto h-6 rounded-[6px] px-2 text-[11.5px] font-medium text-muted-foreground hover:bg-accent-card hover:text-foreground"
          >
            Stop
          </button>
        </Strip>
      )}
      {status.kind === "approval" && thread.approval && (
        <Strip edge="top" tall>
          <div className="flex items-center gap-2">
            <SquareTerminalIcon className="size-[13px] text-warning" />
            <span className="text-[12px] font-semibold text-warning">
              {thread.approval.verb}
            </span>
            <button
              type="button"
              onClick={() => onAnswer(false)}
              className="ml-auto h-6 rounded-[6px] bg-accent-card px-2 text-[11.5px] font-medium hover:bg-accent-card-stronger"
            >
              Refuse
            </button>
            <button
              type="button"
              onClick={() => onAnswer(true)}
              className="h-6 rounded-[6px] bg-warning px-2 text-[11.5px] font-medium text-background hover:brightness-110"
            >
              Allow
            </button>
          </div>
          <p className="mt-2 font-mono text-[12px] break-words">
            {thread.approval.target}
          </p>
        </Strip>
      )}
      <div className="relative z-10 rounded-[22px] border-border-card bg-card">
        <textarea
          ref={input}
          value={text}
          rows={2}
          onChange={(event) => setText(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== "Enter" || event.shiftKey) return
            event.preventDefault()
            send()
          }}
          placeholder={
            status.kind === "working" ? "Send a follow-up" : "Ask anything"
          }
          className="block w-full resize-none bg-transparent px-3.5 pt-3 text-[14px] leading-5 outline-none pointer-coarse:text-[16px] placeholder:text-muted-more-foreground"
        />
        <div
          onClick={(event) => {
            if (event.target !== event.currentTarget) return
            input.current?.focus()
          }}
          className="flex cursor-text items-center px-1.5 py-2"
        >
          <Control icon={<AgentIcon agent={thread.agent} size={14} />}>
            {thread.model}
          </Control>
          <span className="mx-1 h-3.5 w-px bg-border" />
          <Control>High</Control>
          <span className="mx-1 h-3.5 w-px bg-border" />
          <Control
            icon={
              thread.approval ? (
                <ShieldIcon className="size-[13px]" />
              ) : (
                <LockOpenIcon className="size-[13px]" />
              )
            }
          >
            {thread.approval ? "Supervised" : "Full access"}
          </Control>
          <span className="ml-auto flex size-7 cursor-default items-center justify-center rounded-full text-muted-foreground hover:bg-accent-card hover:text-foreground">
            <PaperclipIcon className="size-3.5" />
          </span>
          <button
            type="button"
            onClick={send}
            aria-label="Send"
            className={cn(
              "ml-1 flex size-7 cursor-default items-center justify-center rounded-full bg-primary text-primary-foreground",
              text.trim() ? "hover:brightness-110" : "opacity-45"
            )}
          >
            <ArrowUpIcon className="size-3.5" />
          </button>
        </div>
      </div>
      <Strip edge="bottom">
        <span className="ml-3.5 flex items-center gap-1.5">
          <ServerIcon className="size-[11px]" />
          {thread.server.name}
        </span>
        <span className="mx-2.5 h-3 w-px bg-border" />
        <span className="flex items-center gap-1.5">
          <ProjectIcon project={thread.project} size={13} />
          {thread.project.name}
        </span>
        <span className="ml-auto flex items-center gap-1.5">
          {thread.worktree ? (
            <FolderGit2Icon className="size-[11px]" />
          ) : (
            <FolderIcon className="size-[11px]" />
          )}
          {thread.worktree ? "Worktree" : "Local checkout"}
        </span>
        <span className="mx-2.5 h-3 w-px bg-border" />
        <span className="mr-3.5 flex items-center gap-[5px] text-[11.5px] font-medium">
          <GitBranchIcon className="size-[13px]" />
          {thread.branch}
          {!thread.worktree && (
            <MenuChevron />
          )}
        </span>
      </Strip>
    </div>
  )
}

function Control({
  icon,
  children,
}: {
  icon?: ReactNode
  children: ReactNode
}) {
  return (
    <span
      className={cn(
        "flex h-7 cursor-default items-center gap-1.5 rounded-[7px] pr-[9px] text-[12px] font-medium text-muted-foreground hover:bg-accent-card hover:text-foreground",
        icon ? "pl-[9px]" : "pl-[11px]"
      )}
    >
      <span className="flex items-center gap-[5px]">
        {icon}
        {children}
      </span>
      <MenuChevron />
    </span>
  )
}

/** A strip above or under the composer, narrower than it, with its outer corners rounded. */
function Strip({
  edge,
  tall,
  children,
}: {
  edge: "top" | "bottom"
  tall?: boolean
  children: ReactNode
}) {
  return (
    <div
      className={cn(
        "mx-[22px] border-border-card bg-card text-[12px] text-muted-foreground",
        tall
          ? "p-3 text-[12.5px]"
          : "flex h-8 items-center",
        edge === "top"
          ? "-mb-px rounded-t-[14px] text-foreground"
          : "-mt-px rounded-b-[14px]"
      )}
    >
      {children}
    </div>
  )
}
