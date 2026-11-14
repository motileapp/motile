import {
  ArrowUpIcon,
  BotIcon,
  EyeIcon,
  FolderGit2Icon,
  FolderIcon,
  GitBranchIcon,
  LockIcon,
  PlusIcon,
  ServerIcon,
  ShieldOffIcon,
  SquareTerminalIcon,
} from "lucide-react"
import { Fragment, useRef, useState, type ReactNode } from "react"
import { AgentIcon, EffortHighIcon, MenuChevron, ProjectIcon } from "./icons"
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
  const working = thread.items
    .flatMap((item) => (item.kind === "agents" ? item.agents : []))
    .filter((run) => run.state === "working")
  const send = () => {
    if (!text.trim()) return
    onSend(text.trim())
    setText("")
  }

  return (
    <div className="mx-auto w-full max-w-[796px]">
      {status.kind === "monitoring" && (
        <Strip edge="top">
          <EyeIcon className="mr-1 ml-[10.5px] size-3.5 shrink-0 text-foreground" />
          <span className="text-(length:--type-xs) font-medium text-foreground tabular-nums">
            Monitoring: {elapsed(status.since)}
          </span>
          <button
            type="button"
            onClick={onStop}
            className="mr-[2px] ml-auto h-8 rounded-md px-[11px] text-(length:--type-xs) font-medium text-muted-foreground hover:bg-border-composer hover:text-foreground"
          >
            Stop
          </button>
        </Strip>
      )}
      {(status.kind === "agents" || status.kind === "working") &&
        working.length > 0 && (
          <Strip edge="top">
            <button
              type="button"
              className="ml-[3px] flex h-8 min-w-0 items-center gap-1 rounded-md pr-[11px] pl-[7.5px] text-(length:--type-xs) font-medium text-process hover:bg-border-composer"
            >
              <BotIcon className="size-3.5 shrink-0" />
              <span className="truncate">{working.length} working</span>
            </button>
            {status.kind === "agents" && (
              <button
                type="button"
                className="mr-[2px] ml-auto h-8 shrink-0 rounded-md px-[11px] text-(length:--type-xs) font-medium text-muted-foreground hover:bg-border-composer hover:text-foreground"
              >
                Stop
              </button>
            )}
          </Strip>
        )}
      {status.kind === "approval" && thread.approval && (
        <Strip edge="top" tall>
          <div className="flex items-center gap-2">
            <SquareTerminalIcon className="size-3.5 text-warning" />
            <span className="text-(length:--type-xs) font-semibold text-warning">
              {thread.approval.verb}
            </span>
          </div>
          <p className="mt-3 rounded-md bg-border-composer px-2.5 py-1.5 font-mono text-(length:--type-xs) break-words">
            <Command text={thread.approval.target} />
          </p>
          <div className="mt-3 flex justify-end gap-2">
            <button
              type="button"
              onClick={() => onAnswer(false)}
              className="h-8 rounded-md bg-border-composer px-[11px] text-(length:--type-xs) font-medium"
            >
              Refuse
            </button>
            <button
              type="button"
              onClick={() => onAnswer(true)}
              className="h-8 rounded-md bg-warning px-[11px] text-(length:--type-xs) font-medium text-warning-foreground hover:bg-warning/(--opacity-lit)"
            >
              Allow
            </button>
          </div>
        </Strip>
      )}
      <div className="relative z-10 rounded-2xl border border-border-composer bg-composer">
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
          className="block w-full resize-none bg-transparent px-3.5 pt-3 text-(length:--type-md) leading-5 outline-none placeholder:text-muted-stronger-foreground pointer-coarse:text-(length:--type-lg)"
        />
        <div
          onClick={(event) => {
            if (event.target !== event.currentTarget) return
            input.current?.focus()
          }}
          className="flex cursor-text items-center px-1.5 py-2"
        >
          <span className="mx-[3px] flex size-8 cursor-default items-center justify-center rounded-full text-foreground hover:bg-border-composer">
            <PlusIcon className="size-[18px]" />
          </span>
          <span className="mr-0.5 ml-auto flex h-8 cursor-default items-center rounded-md px-[11px] text-(length:--type-xs) font-medium text-muted-foreground hover:bg-border-composer hover:text-foreground">
            <Setting icon={<AgentIcon agent={thread.agent} size={14} />}>
              <Dotted text={thread.model} />
            </Setting>
            <SettingDivider />
            <Setting icon={<EffortHighIcon />}>High</Setting>
            <SettingDivider />
            <Setting
              icon={
                thread.approval ? (
                  <LockIcon className="size-3.5" />
                ) : (
                  <ShieldOffIcon className="size-3.5" />
                )
              }
            >
              {thread.approval ? "Supervised" : "Full access"}
            </Setting>
          </span>
          <button
            type="button"
            onClick={send}
            aria-label="Send"
            className={cn(
              "flex size-8 cursor-default items-center justify-center rounded-full bg-primary text-primary-foreground",
              text.trim()
                ? "hover:bg-primary/(--opacity-lit)"
                : "opacity-(--opacity-disabled)"
            )}
          >
            <ArrowUpIcon className="size-4" />
          </button>
        </div>
      </div>
      <Strip edge="bottom">
        <span className="ml-3.5 flex items-center gap-1.5 font-medium">
          <ServerIcon className="size-3.5" />
          {thread.server.name}
        </span>
        <span className="mx-2.5 h-3 w-px bg-border-composer" />
        <span className="flex items-center gap-1.5 font-medium">
          <ProjectIcon project={thread.project} />
          {thread.project.name}
        </span>
        <span className="ml-auto flex items-center gap-1.5 font-medium">
          {thread.worktree ? (
            <FolderGit2Icon className="size-3.5" />
          ) : (
            <FolderIcon className="size-3.5" />
          )}
          {thread.worktree ? "Worktree" : "Local checkout"}
        </span>
        <span className="ml-2.5 h-3 w-px bg-border-composer" />
        <span className="mr-[2px] flex items-center">
          <Control
            icon={<GitBranchIcon className="size-3.5" />}
            plain={!!thread.worktree}
          >
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
        "flex h-8 cursor-default items-center gap-1.5 rounded-md text-(length:--type-xs) font-medium text-muted-foreground",
        !plain && "hover:bg-border-composer hover:text-foreground",
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

/** A part of the button that picks the model, its effort and the access: one button, as on the Mac. */
function Setting({ icon, children }: { icon: ReactNode; children: ReactNode }) {
  return (
    <span className="flex items-center gap-[5px]">
      {icon}
      {children}
    </span>
  )
}

/** The line between two of its parts, as far from the words before it as from the symbol after it. */
function SettingDivider() {
  return <span className="mr-[13px] ml-2 h-3.5 w-px bg-border-composer" />
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
        "mx-[22px] border border-border-composer bg-composer text-(length:--type-xs) text-muted-foreground",
        tall ? "p-3 text-(length:--type-xs)" : "flex h-[39px] items-center",
        edge === "top"
          ? cn(
              "-mb-px rounded-t-lg text-foreground",
              tall ? "pb-[13px]" : "pb-px"
            )
          : cn("-mt-px rounded-b-lg", tall ? "pt-[13px]" : "pt-px")
      )}
    >
      {children}
    </div>
  )
}

/** A command with its program in the function colour, as the core's highlighter colours it. */
function Command({ text }: { text: string }) {
  const space = text.indexOf(" ")
  if (space < 0) return <span className="text-syntax-function">{text}</span>
  return (
    <>
      <span className="text-syntax-function">{text.slice(0, space)}</span>
      {text.slice(space)}
    </>
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
