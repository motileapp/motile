import {
  ChevronDownIcon,
  ChevronRightIcon,
  DiffIcon,
  FileTextIcon,
  FoldVerticalIcon,
  PlusIcon,
  RotateCwIcon,
  SquareArrowOutUpRightIcon,
  UnfoldVerticalIcon,
  XIcon,
} from "lucide-react"
import { useState } from "react"
import { parseDiff, type DiffFile } from "./diff"
import { highlight } from "./highlight"
import { MenuChevron } from "./icons"
import { folderOf, LineCounts, nameOf } from "./transcript"
import { cn } from "@/lib/utils"

/** The side panel with one tab open, the thread's uncommitted changes. */
export function DiffPanel({
  diff,
  pastWindowButtons,
  onClose,
}: {
  diff: string
  pastWindowButtons: boolean
  onClose: () => void
}) {
  const files = parseDiff(diff)
  const [closed, setClosed] = useState<string[]>([])
  const allClosed = closed.length >= files.length
  const added = files.reduce((sum, file) => sum + file.added, 0)
  const removed = files.reduce((sum, file) => sum + file.removed, 0)
  const toggle = (path: string) =>
    setClosed(
      closed.includes(path)
        ? closed.filter((one) => one !== path)
        : [...closed, path]
    )

  return (
    <div className="flex h-full min-w-0 flex-col">
      <div
        className={cn(
          "flex h-[52px] shrink-0 items-center gap-0.5 pr-[74px]",
          pastWindowButtons ? "pl-[200px]" : "pl-2"
        )}
      >
        <span className="flex h-7 items-center gap-1.5 rounded-[7px] bg-selected pr-0.5 pl-[9px] text-[12px] font-medium">
          <DiffIcon className="size-[11px]" />
          Diff
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="flex size-6 items-center justify-center rounded-[6px] text-muted-foreground hover:bg-hover hover:text-foreground"
          >
            <XIcon className="size-[13px]" />
          </button>
        </span>
        <span className="flex size-7 items-center justify-center rounded-[7px] text-muted-foreground hover:bg-hover hover:text-foreground">
          <PlusIcon className="size-3.5" />
        </span>
      </div>
      <div className="flex h-9 shrink-0 items-center gap-1 border-t border-b pr-1 pl-3">
        <span className="-ml-2 flex h-7 items-center gap-1.5 rounded-[7px] px-[11px] text-[12px] font-medium text-muted-foreground hover:bg-hover hover:text-foreground">
          Uncommitted
          <MenuChevron />
        </span>
        {files.length > 0 && (
          <span className="ml-1">
            <LineCounts added={added} removed={removed} />
          </span>
        )}
        {files.length > 1 && (
          <button
            type="button"
            onClick={() =>
              setClosed(allClosed ? [] : files.map((file) => file.path))
            }
            aria-label={allClosed ? "Open every file" : "Close every file"}
            className="ml-auto flex size-7 items-center justify-center rounded-[7px] text-muted-foreground hover:bg-hover hover:text-foreground"
          >
            {allClosed ? (
              <UnfoldVerticalIcon className="size-3.5" />
            ) : (
              <FoldVerticalIcon className="size-3.5" />
            )}
          </button>
        )}
        <span
          className={cn(
            "flex size-7 items-center justify-center rounded-[7px] text-muted-foreground hover:bg-hover hover:text-foreground",
            files.length < 2 && "ml-auto"
          )}
        >
          <RotateCwIcon className="size-3.5" />
        </span>
      </div>
      {files.length === 0 ? (
        <p className="m-auto text-[12.5px] text-muted-foreground">
          Everything is committed.
        </p>
      ) : (
        <div className="min-h-0 flex-1 overflow-auto">
          {files.map((file, index) => (
            <File
              key={file.path}
              file={file}
              first={index === 0}
              closed={closed.includes(file.path)}
              onToggle={() => toggle(file.path)}
            />
          ))}
        </div>
      )}
    </div>
  )
}

const LINE_FILLS = {
  added: "bg-added",
  removed: "bg-removed",
  note: "bg-hover text-muted-foreground",
  unchanged: "",
}

function File({
  file,
  first,
  closed,
  onToggle,
}: {
  file: DiffFile
  first: boolean
  closed: boolean
  onToggle: () => void
}) {
  return (
    <div className={cn(!first && "mt-3")}>
      <button
        type="button"
        onClick={onToggle}
        className={cn(
          "flex h-[34px] w-full items-center border-b bg-bubble pr-1.5 text-left text-[12.5px]",
          !first && "border-t"
        )}
      >
        <span className="flex w-7 justify-center text-tertiary">
          {closed ? (
            <ChevronRightIcon className="size-[9px]" />
          ) : (
            <ChevronDownIcon className="size-[9px]" />
          )}
        </span>
        <FileTextIcon className="mr-2 size-[11px] shrink-0 text-muted-foreground" />
        <span className="truncate">
          <span className="text-muted-foreground">{folderOf(file.path)}</span>
          <span className="font-medium">{nameOf(file.path)}</span>
          {file.isNew && (
            <span className="ml-3 text-[11.5px] text-tertiary">new</span>
          )}
        </span>
        <span className="mr-1 ml-auto pl-2.5">
          <LineCounts added={file.added} removed={file.removed} />
        </span>
        <span className="flex size-7 items-center justify-center text-muted-foreground">
          <SquareArrowOutUpRightIcon className="size-3" />
        </span>
      </button>
      {!closed && (
        <div className="min-w-max font-mono text-[12.5px] leading-[18px]">
          {file.lines.map((line, index) => (
            <div key={index} className={cn("flex", LINE_FILLS[line.kind])}>
              <span className="w-9 shrink-0 pr-1.5 text-right text-[11px] text-tertiary tabular-nums select-none">
                {line.old}
              </span>
              <span className="w-9 shrink-0 pr-1.5 text-right text-[11px] text-tertiary tabular-nums select-none">
                {line.new}
              </span>
              <span className="pr-3 pl-3 whitespace-pre">
                {line.kind === "note" ? line.text : highlight(line.text)}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
