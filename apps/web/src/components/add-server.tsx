import { useEffect, useState } from "react"
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

  async function start() {
    setInstall(undefined)
    setOpen(true)
    try {
      setInstall(await createInstallCommand())
    } catch (error) {
      setOpen(false)
      toast.error(error instanceof Error ? error.message : String(error))
      await router.invalidate()
    }
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
            <DialogTitle>Add a server</DialogTitle>
            <DialogDescription>
              Run this on the Linux machine or Mac that will run your agents.
            </DialogDescription>
          </DialogHeader>
          {install ? (
            <Command text={install.command} />
          ) : (
            <div className="flex h-24 items-center justify-center">
              <Spinner />
            </div>
          )}
          <p className="flex items-center gap-2 text-sm text-muted-foreground">
            <Spinner className="size-3.5" />
            Waiting for your server. The command works once, for an hour.
          </p>
        </DialogContent>
      </Dialog>
    </>
  )
}

function Command({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)

  async function copy() {
    await navigator.clipboard.writeText(text)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <div className="flex items-start gap-2 rounded-2xl bg-background p-3 pl-4">
      <code className="min-w-0 flex-1 py-1.5 font-mono text-xs leading-relaxed break-all">
        {text}
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
