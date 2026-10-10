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
import { Fragment, useRef, useState, type ReactNode } from "react"
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
            className="mr-2 ml-auto h-7 rounded-md px-[11px] text-[12px] font-medium text-muted-foreground hover:bg-background-accent hover:text-foreground"
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
              className="ml-auto h-6 rounded-sm bg-background-accent px-2 text-[11.5px] font-medium hover:bg-background-accent-stronger"
            >
              Refuse
            </button>
            <button
              type="button"
              onClick={() => onAnswer(true)}
              className="h-6 rounded-sm bg-warning px-2 text-[11.5px] font-medium text-warning-foreground hover:bg-warning/(--opacity-lit)"
            >
              Allow
            </button>
          </div>
          <p className="mt-2 font-mono text-[12px] break-words">
            {thread.approval.target}
          </p>
        </Strip>
      )}
      <div className="relative z-10 rounded-2xl border border-border-input bg-composer">
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
          className="block w-full resize-none bg-transparent px-3.5 pt-3 text-[14px] leading-5 outline-none pointer-coarse:text-[16px] placeholder:text-muted-stronger-foreground"
        />
        <div
          onClick={(event) => {
            if (event.target !== event.currentTarget) return
            input.current?.focus()
          }}
          className="flex cursor-text items-center px-1.5 py-2"
        >
          <Control icon={<AgentIcon agent={thread.agent} size={14} />}>
            <Dotted text={thread.model} />
          </Control>
          <span className="mx-1 h-3.5 w-px bg-border-input" />
          <Control>High</Control>
          <span className="mx-1 h-3.5 w-px bg-border-input" />
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
          <span className="ml-auto flex size-7 cursor-default items-center justify-center rounded-full text-muted-foreground hover:bg-background-accent hover:text-foreground">
            <PaperclipIcon className="size-3.5" />
          </span>
          <button
            type="button"
            onClick={send}
            aria-label="Send"
            className={cn(
              "ml-1 flex size-7 cursor-default items-center justify-center rounded-full bg-primary text-primary-foreground",
              text.trim() ? "hover:bg-primary/(--opacity-lit)" : "opacity-(--opacity-disabled)"
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
        <span className="mx-2.5 h-3 w-px bg-border-input" />
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
        <span className="ml-2.5 h-3 w-px bg-border-input" />
        <span className="mr-2 flex items-center">
          <Control icon={<GitBranchIcon className="size-3.5" />} plain={!!thread.worktree}>
            {thread.branch}
          </Control>
        </span>
      </Strip>
    </div>
  )
}

/** A control of the composer's rows. A plain one is a label set like the menus beside it. */
function Control({
  icon,
  plain,
  children,
}: {
  icon?: ReactNode
  plain?: boolean
  children: ReactNode
}) {
  return (
    <span
      className={cn(
        "flex h-7 cursor-default items-center gap-1.5 rounded-md text-[12px] font-medium text-muted-foreground",
        !plain && "hover:bg-background-accent hover:text-foreground",
        icon ? "pl-[9px]" : "pl-[11px]",
        plain ? "pr-[11px]" : "pr-[9px]"
      )}
    >
      <span className="flex items-center gap-[5px]">
        {icon}
        {children}
      </span>
      {!plain && <MenuChevron />}
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
        "mx-[22px] border border-border-input bg-composer text-[12px] text-muted-foreground",
        tall
          ? "p-3 text-[12.5px]"
          : "flex h-[35px] items-center",
        edge === "top"
          ? "-mb-px rounded-t-lg pb-px text-foreground"
          : "-mt-px rounded-b-lg pt-px"
      )}
    >
      {children}
    </div>
  )
}

/** The text with the dots between its parts muted, as "Opus 5.5 · Work". */
export function Dotted({ text }: { text: string }) {
  return text.split(" · ").map((part, index) => (
    <Fragment key={index}>
      {index > 0 && <span className="text-muted-stronger-foreground"> · </span>}
      {part}
    </Fragment>
  ))
}
