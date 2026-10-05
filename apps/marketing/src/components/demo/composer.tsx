import {
  ArrowUpIcon,
  ChevronDownIcon,
  FolderGit2Icon,
  FolderIcon,
  GitBranchIcon,
  LockOpenIcon,
  PaperclipIcon,
  ServerIcon,
  ShieldIcon,
  SquareTerminalIcon,
} from "lucide-react"
import { useState, type ReactNode } from "react"
import { AgentIcon, ProjectIcon } from "./icons"
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
          <span className="text-[12.5px] font-medium tabular-nums">
            Monitoring for {elapsed(status.since)}
          </span>
          <button
            type="button"
            onClick={onStop}
            className="mr-1 ml-auto h-6 rounded-[7px] px-[9px] text-[12.5px] font-medium hover:bg-hover"
          >
            Stop
          </button>
        </Strip>
      )}
      <div className="relative z-10 rounded-[22px] border bg-composer">
        {status.kind === "approval" && thread.approval && (
          <div className="mx-2.5 mt-2.5 rounded-xl bg-warning-background p-3 text-[12.5px]">
            <p className="text-[12px] font-semibold text-warning">
              Waiting for you
            </p>
            <div className="mt-2 flex items-center gap-2">
              <SquareTerminalIcon className="size-[13px] text-muted-foreground" />
              <span className="font-medium">{thread.approval.verb}</span>
              <span className="truncate font-mono text-[12px]">
                {thread.approval.target}
              </span>
              <button
                type="button"
                onClick={() => onAnswer(false)}
                className="ml-auto h-[22px] rounded-md border border-strong-border bg-composer px-2.5 text-[12px] hover:bg-hover"
              >
                Deny
              </button>
              <button
                type="button"
                onClick={() => onAnswer(true)}
                className="h-[22px] rounded-md bg-primary px-2.5 text-[12px] text-primary-foreground hover:bg-primary/85"
              >
                Allow
              </button>
            </div>
          </div>
        )}
        <textarea
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
          className="block w-full resize-none bg-transparent px-3.5 pt-3 text-[14px] leading-5 outline-none pointer-coarse:text-[16px] placeholder:text-tertiary"
        />
        <div className="flex items-center px-1.5 py-2">
          <Control>
            <AgentIcon agent={thread.agent} size={14} />
            {thread.model}
          </Control>
          <Control>High</Control>
          <Control>
            {thread.approval ? (
              <>
                <ShieldIcon className="size-[13px]" />
                Supervised
              </>
            ) : (
              <>
                <LockOpenIcon className="size-[13px]" />
                Full access
              </>
            )}
          </Control>
          <span className="ml-auto flex size-[30px] items-center justify-center rounded-full text-muted-foreground hover:bg-hover">
            <PaperclipIcon className="size-[15px]" />
          </span>
          <button
            type="button"
            onClick={send}
            aria-label="Send"
            className={cn(
              "ml-1 flex size-[30px] items-center justify-center rounded-full",
              text.trim()
                ? "bg-primary text-primary-foreground"
                : "bg-hover text-muted-foreground"
            )}
          >
            <ArrowUpIcon className="size-4" />
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
        <span className="mx-2.5 h-3 w-px bg-border" />
        <span className="flex items-center gap-1.5">
          {thread.worktree ? (
            <FolderGit2Icon className="size-[11px]" />
          ) : (
            <FolderIcon className="size-[11px]" />
          )}
          {thread.worktree ? "Worktree" : "Local checkout"}
        </span>
        <span className="mr-3.5 ml-auto flex items-center gap-1.5">
          <GitBranchIcon className="size-[11px]" />
          {thread.branch}
          {!thread.worktree && (
            <ChevronDownIcon className="size-[9px] text-tertiary" />
          )}
        </span>
      </Strip>
    </div>
  )
}

function Control({ children }: { children: ReactNode }) {
  return (
    <span className="flex h-[30px] items-center gap-1.5 rounded-[15px] px-[9px] text-[12.5px] font-medium hover:bg-hover">
      {children}
      <ChevronDownIcon className="size-[9px] text-tertiary" />
    </span>
  )
}

/** A strip above or under the composer, narrower than it, with its outer corners rounded. */
function Strip({
  edge,
  children,
}: {
  edge: "top" | "bottom"
  children: ReactNode
}) {
  return (
    <div
      className={cn(
        "mx-[22px] flex h-8 items-center border bg-composer text-[12px] text-muted-foreground",
        edge === "top"
          ? "-mb-px rounded-t-[14px] text-foreground"
          : "-mt-px rounded-b-[14px]"
      )}
    >
      {children}
    </div>
  )
}
