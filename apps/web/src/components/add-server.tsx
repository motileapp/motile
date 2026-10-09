import { Fragment, useEffect, useState } from "react"
import { useRouter } from "@tanstack/react-router"
import { CheckIcon, CopyIcon, PlusIcon } from "lucide-react"
import { toast } from "sonner"
import type { Device, InstallCommand } from "@/lib/account"
import { createInstallCommand, getAccount } from "@/lib/account"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Spinner } from "@/components/ui/spinner"

const CHECK_EVERY_MS = 3000

export function AddServer({ servers }: { servers: Array<Device> }) {
  const router = useRouter()
  const [open, setOpen] = useState(false)
  const [install, setInstall] = useState<InstallCommand>()
  const [pending, setPending] = useState(false)
  const now = useNow(open)
  const expired = install !== undefined && install.expires_at * 1000 <= now

  async function generate() {
    setPending(true)
    try {
      setInstall(await createInstallCommand())
    } catch (error) {
      setOpen(false)
      toast.error(error instanceof Error ? error.message : String(error))
      await router.invalidate()
    } finally {
      setPending(false)
    }
  }

  async function start() {
    setInstall(undefined)
    setOpen(true)
    await generate()
  }

  // The dialog closes by itself once the command has linked a machine.
  useEffect(() => {
    if (!open) return
    const known = new Set(servers.map((server) => server.public_key))
    const timer = setInterval(async () => {
      const account = await getAccount().catch(() => null)
      const added = account?.servers.find((server) => !known.has(server.public_key))
      if (!added) return
      setOpen(false)
      toast.success(`${added.name} was added.`)
      await router.invalidate()
    }, CHECK_EVERY_MS)
    return () => clearInterval(timer)
  }, [open, servers, router])

  return (
    <>
      <Button onClick={start}>
        <PlusIcon data-icon="inline-start" />
        Add a server
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Run this on your server</DialogTitle>
            <DialogDescription>
              Run the command below on the server that will run your agents
            </DialogDescription>
          </DialogHeader>
          {install ? (
            <Command text={install.command} />
          ) : (
            <div className="flex h-24 items-center justify-center">
              <Spinner />
            </div>
          )}
          {expired ? (
            <div className="flex items-center justify-between gap-2 text-sm text-muted-foreground">
              The command has expired
              <Button variant="secondary" size="sm" disabled={pending} onClick={generate}>
                {pending ? <Spinner data-icon="inline-start" /> : null}
                Regenerate
              </Button>
            </div>
          ) : (
            <div className="flex items-center justify-between gap-2 text-sm text-muted-foreground">
              <span className="flex items-center gap-2">
                <Spinner className="size-3.5" />
                Waiting for your server
              </span>
              <span className="tabular-nums">
                {install ? timeLeft(install.expires_at, now) : "15:00"}
              </span>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </>
  )
}

// The time of day, once a second, while something counts on it.
function useNow(active: boolean) {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    if (!active) return
    setNow(Date.now())
    const timer = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(timer)
  }, [active])
  return now
}

function timeLeft(expiresAt: number, now: number) {
  const seconds = Math.max(0, Math.ceil(expiresAt - now / 1000))
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`
}

function Command({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)

  async function copy() {
    await navigator.clipboard.writeText(text)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <div className="flex items-start gap-2 rounded-lg border bg-background-secondary p-3 pl-4">
      <code className="min-w-0 flex-1 py-1.5 font-mono text-xs leading-relaxed break-all">
        {colouredWords(text).map(({ word, colour }, index) => (
          <Fragment key={index}>
            {index > 0 && " "}
            <span className={colour}>{word}</span>
          </Fragment>
        ))}
      </code>
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label="Copy the command"
        onClick={copy}
      >
        {copied ? <CheckIcon /> : <CopyIcon />}
      </Button>
    </div>
  )
}

const OPERATORS = new Set(["|", "&&", "||", ";"])

// The command's words in the theme's code colours, as the core colours them for the clients.
function colouredWords(command: string) {
  let expectsProgram = true
  return command.split(" ").map((word) => {
    const isOperator = OPERATORS.has(word)
    const isVariable = !isOperator && expectsProgram && word.includes("=")
    const colour = isOperator
      ? "text-syntax-keyword"
      : isVariable
        ? "text-syntax-type"
        : expectsProgram
          ? "text-syntax-function"
          : word.startsWith("-")
            ? "text-syntax-constant"
            : word.includes("://")
              ? "text-syntax-string"
              : "text-syntax-type"
    if (word) expectsProgram = isOperator || isVariable
    return { word, colour }
  })
}
