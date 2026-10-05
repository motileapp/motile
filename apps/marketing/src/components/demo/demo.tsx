import {
  ChevronDownIcon,
  FolderPlusIcon,
  GitCommitHorizontalIcon,
  Maximize2Icon,
  Minimize2Icon,
  PanelLeftIcon,
  PanelRightIcon,
  SquarePenIcon,
  type LucideIcon,
} from "lucide-react"
import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type RefObject,
} from "react"
import { Composer } from "./composer"
import { DiffPanel } from "./diff-panel"
import { Glow } from "./glow"
import { ProjectIcon, TrafficLights } from "./icons"
import { Sidebar } from "./sidebar"
import { threads as startingThreads, type Item, type Thread } from "./threads"
import { Transcript } from "./transcript"
import { cn } from "@/lib/utils"

type Panel = "closed" | "open" | "maximized"

/** The composer's height with no strip above it, so the page paints right before it is measured. */
const IDLE_COMPOSER_HEIGHT = 171

/** The Mac app's window, with made-up threads that can be opened, answered and written in. */
export function Demo() {
  const [threads, setThreads] = useState(startingThreads)
  const [selectedId, setSelectedId] = useState(startingThreads[0].id)
  const [panel, setPanel] = useState<Panel>("closed")
  const [sidebarShown, setSidebarShown] = useState(true)
  const seconds = useSeconds()
  const composer = useRef<HTMLDivElement>(null)
  const composerRoom = useHeight(composer, IDLE_COMPOSER_HEIGHT)

  const thread = threads.find((one) => one.id === selectedId) ?? threads[0]
  const elapsed = (since: number) => formatElapsed(since + seconds)
  const update = (id: string, change: (thread: Thread) => Partial<Thread>) =>
    setThreads((all) =>
      all.map((one) => (one.id === id ? { ...one, ...change(one) } : one))
    )
  const finish = (id: string, items: Item[]) =>
    update(id, (one) => ({
      items: [...one.items, ...items],
      status: { kind: "idle", ago: "now" },
    }))

  const send = (text: string) => {
    const id = thread.id
    update(id, (one) => ({
      items: [...one.items, { kind: "user", text }],
      status: { kind: "working", since: -seconds },
    }))
    setTimeout(() => {
      const agent = thread.agent === "claude" ? "Claude Code" : "Codex"
      finish(id, [
        {
          kind: "text",
          text: `This is a demo, so nothing ran. On your own server, ${agent} would be on it now, and the thread would keep going with the window closed.`,
        },
        { kind: "end", worked: "Worked for 2s" },
      ])
    }, 2400)
  }

  const answer = (allow: boolean) => {
    if (!thread.approval) return
    finish(thread.id, allow ? thread.approval.allowed : thread.approval.denied)
  }

  const togglePanel = () => setPanel(panel === "closed" ? "open" : "closed")

  return (
    <div className="@container relative isolate w-full">
      <Glow />
      <div className="h-[calc(760px*var(--demo-scale))] [--demo-scale:var(--demo-fit,min(1,tan(atan2(100cqw,1200px))))]">
        <div className="relative flex h-[760px] w-[1200px] origin-top-left scale-(--demo-scale) overflow-hidden rounded-[16px] bg-background font-system text-[13px] text-foreground ring-1 ring-black/10 select-none dark:ring-white/12">
          <div className="absolute top-5 left-5 z-20">
            <TrafficLights />
          </div>
          <div className="absolute top-2.5 left-[74px] z-20 flex">
            <WindowButton
              icon={PanelLeftIcon}
              label="Show or hide the sidebar"
              onClick={() => setSidebarShown(!sidebarShown)}
            />
            {!sidebarShown && <ProjectButtons />}
          </div>
          <div className="absolute top-2.5 right-2.5 z-20 flex">
            {panel !== "closed" && (
              <WindowButton
                icon={panel === "maximized" ? Minimize2Icon : Maximize2Icon}
                label="Maximize or restore the side panel"
                onClick={() =>
                  setPanel(panel === "maximized" ? "open" : "maximized")
                }
              />
            )}
            <WindowButton
              icon={PanelRightIcon}
              label="Show or hide the side panel"
              onClick={togglePanel}
            />
          </div>

          {sidebarShown && (
            <aside className="flex w-[268px] shrink-0 flex-col border-r">
              <div className="flex h-[52px] shrink-0 justify-end px-2 pt-2.5">
                <ProjectButtons />
              </div>
              <div className="min-h-0 flex-1">
                <Sidebar
                  threads={threads}
                  selectedId={thread.id}
                  elapsed={elapsed}
                  onSelect={setSelectedId}
                />
              </div>
            </aside>
          )}

          <main
            className={cn(
              "flex min-w-0 flex-1 flex-col",
              panel === "maximized" && "hidden"
            )}
          >
            <div
              className={cn(
                "flex h-[52px] shrink-0 items-center gap-2",
                sidebarShown ? "pl-5" : "pl-[200px]",
                panel === "closed" ? "pr-[54px]" : "pr-2"
              )}
            >
              <div className="min-w-0">
                <p className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
                  <ProjectIcon project={thread.project} />
                  {thread.project.name} · {thread.branch}
                </p>
                <p className="truncate text-[13px] font-semibold">
                  {thread.title}
                </p>
              </div>
              {thread.diff && <GitButton />}
            </div>
            <div className="relative min-h-0 flex-1">
              <div
                className="absolute inset-0 flex flex-col-reverse overflow-y-auto"
                style={fadeUnder(composerRoom)}
              >
                <div
                  className="mb-auto"
                  style={{ paddingBottom: composerRoom }}
                >
                  <Transcript
                    key={thread.id}
                    thread={thread}
                    elapsed={
                      thread.status.kind === "working"
                        ? elapsed(thread.status.since)
                        : ""
                    }
                    onOpenDiff={() => setPanel("open")}
                  />
                </div>
              </div>
              <div
                ref={composer}
                className="pointer-events-none absolute inset-x-0 bottom-0 px-6 pt-6 pb-4 *:pointer-events-auto"
              >
                <Composer
                  key={thread.id}
                  thread={thread}
                  elapsed={elapsed}
                  onSend={send}
                  onAnswer={answer}
                  onStop={() =>
                    update(thread.id, () => ({
                      status: { kind: "idle", ago: "now" },
                    }))
                  }
                />
              </div>
            </div>
          </main>

          {panel !== "closed" && (
            <section
              className={cn(
                "flex min-w-0 flex-col",
                panel === "maximized" ? "flex-1" : "w-[440px] shrink-0 border-l"
              )}
            >
              <DiffPanel
                key={thread.id}
                diff={thread.diff}
                pastWindowButtons={panel === "maximized" && !sidebarShown}
                onClose={() => setPanel("closed")}
              />
            </section>
          )}
        </div>
      </div>
    </div>
  )
}

function ProjectButtons() {
  return (
    <>
      <WindowButton icon={FolderPlusIcon} label="Add a project" />
      <WindowButton icon={SquarePenIcon} label="New thread" />
    </>
  )
}

function WindowButton({
  icon: Icon,
  label,
  onClick,
}: {
  icon: LucideIcon
  label: string
  onClick?: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      title={label}
      className="m-0.5 flex size-7 items-center justify-center rounded-[7px] hover:bg-hover"
    >
      <Icon className="size-[15px]" />
    </button>
  )
}

function GitButton() {
  return (
    <span className="mx-1.5 ml-auto flex h-7 shrink-0 items-center overflow-hidden rounded-[7px] border border-strong-border text-[12px] font-medium">
      <span className="flex h-full items-center gap-1.5 px-[9px] hover:bg-hover">
        <GitCommitHorizontalIcon className="size-3" />
        Commit
      </span>
      <span className="flex h-full w-6 items-center justify-center border-l border-strong-border text-muted-foreground hover:bg-hover">
        <ChevronDownIcon className="size-3" />
      </span>
    </span>
  )
}

/** The transcript fades out from the room above the composer down to the composer's middle, as in the Mac app. */
function fadeUnder(room: number): CSSProperties {
  const middle = (room - 24 + 16) / 2
  const mask = `linear-gradient(to bottom, #000 calc(100% - ${room}px), transparent calc(100% - ${middle}px))`
  return { maskImage: mask }
}

function useHeight(ref: RefObject<HTMLElement | null>, initial: number) {
  const [height, setHeight] = useState(initial)
  useEffect(() => {
    const element = ref.current
    if (!element) return
    const observer = new ResizeObserver(() => setHeight(element.offsetHeight))
    observer.observe(element)
    return () => observer.disconnect()
  }, [ref])
  return height
}

/** Seconds since the page opened, so the timers of working threads count up. */
function useSeconds() {
  const [seconds, setSeconds] = useState(0)
  useEffect(() => {
    const timer = setInterval(() => setSeconds((value) => value + 1), 1000)
    return () => clearInterval(timer)
  }, [])
  return seconds
}

function formatElapsed(seconds: number) {
  if (seconds < 60) return `${seconds}s`
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`
  return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`
}
