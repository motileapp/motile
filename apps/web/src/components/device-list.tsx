import { useState } from "react"
import { useRouter } from "@tanstack/react-router"
import { LaptopIcon, ServerIcon } from "lucide-react"
import { toast } from "sonner"
import type { ComponentProps } from "react"
import type { Device, RemovedServer } from "@/lib/account"
import { deleteServerCompletely, removeDevice } from "@/lib/account"
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemGroup,
  ItemMedia,
  ItemTitle,
} from "@/components/ui/item"

const PLATFORMS: Record<string, string> = {
  macos: "macOS",
  linux: "Linux",
  ios: "iOS",
  android: "Android",
  windows: "Windows",
}

// UTC, so the server and the browser render the same text.
const DAY = new Intl.DateTimeFormat("en", {
  dateStyle: "medium",
  timeZone: "UTC",
})

export function DeviceList({ devices }: { devices: Array<Device> }) {
  return (
    <ItemGroup className="gap-2">
      {devices.map((device) => (
        <Item key={device.public_key} variant="outline" role="listitem">
          <ItemMedia variant="icon">
            {device.kind === "server" ? <ServerIcon /> : <LaptopIcon />}
          </ItemMedia>
          <ItemContent>
            <ItemTitle>{device.name}</ItemTitle>
            <ItemDescription>
              {platformName(device.platform)} · Added{" "}
              {DAY.format(device.created_at * 1000)}
            </ItemDescription>
          </ItemContent>
          <ItemActions>
            <RemoveDevice device={device} />
            {device.kind === "server" && <DeleteCompletely server={device} />}
          </ItemActions>
        </Item>
      ))}
    </ItemGroup>
  )
}

/** The servers removed from the account, which can still be deleted completely. */
export function RemovedServerList({
  servers,
}: {
  servers: Array<RemovedServer>
}) {
  return (
    <div className="flex flex-col gap-2">
      <h3 className="px-2 text-sm font-medium text-muted-foreground">
        Unlinked
      </h3>
      <ItemGroup className="gap-2">
        {servers.map((server) => (
          <Item key={server.public_key} variant="outline" role="listitem">
            <ItemMedia variant="icon">
              <ServerIcon />
            </ItemMedia>
            <ItemContent>
              <ItemTitle>{server.name}</ItemTitle>
              <ItemDescription>
                {platformName(server.platform)} · Unlinked{" "}
                {DAY.format(server.removed_at * 1000)}
              </ItemDescription>
            </ItemContent>
            <ItemActions>
              <DeleteCompletely server={server} />
            </ItemActions>
          </Item>
        ))}
      </ItemGroup>
    </div>
  )
}

function platformName(platform: string) {
  return PLATFORMS[platform] ?? platform
}

function RemoveDevice({ device }: { device: Device }) {
  if (device.kind === "client") {
    return (
      <Confirm
        trigger="Remove"
        title={`Remove ${device.name}?`}
        message="This client is signed out and can no longer reach your servers."
        confirm="Remove"
        done={`${device.name} was removed.`}
        action={() => removeDevice({ data: device.public_key })}
      />
    )
  }
  return (
    <Confirm
      trigger="Unlink"
      title={`Unlink “${device.name}”?`}
      message={`Your account lets go of ${device.name}. Its threads stay readable, and can go on, from the copies your other servers keep. If it is your only server, nothing is kept. What is on it stays there.`}
      confirm="Unlink"
      done={`${device.name} was unlinked.`}
      action={() => removeDevice({ data: device.public_key })}
    />
  )
}

function DeleteCompletely({
  server,
}: {
  server: { public_key: string; name: string }
}) {
  return (
    <Confirm
      trigger="Delete"
      triggerVariant="ghost-destructive"
      title={`Delete “${server.name}”?`}
      message={`${server.name} leaves your account and every copy of its threads is deleted, on all your servers and clients. What is on ${server.name} itself stays there.`}
      confirm="Delete"
      done={`${server.name} was deleted.`}
      action={() => deleteServerCompletely({ data: server.public_key })}
    />
  )
}

function Confirm({
  trigger,
  triggerVariant = "ghost",
  title,
  message,
  confirm,
  done,
  action,
}: {
  trigger: string
  triggerVariant?: ComponentProps<typeof Button>["variant"]
  title: string
  message: string
  confirm: string
  done: string
  action: () => Promise<void>
}) {
  const router = useRouter()
  const [open, setOpen] = useState(false)
  const [pending, setPending] = useState(false)

  async function run() {
    setPending(true)
    try {
      await action()
      toast.success(done)
      setOpen(false)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error))
    }
    setPending(false)
    await router.invalidate()
  }

  return (
    <AlertDialog open={open} onOpenChange={setOpen}>
      <AlertDialogTrigger
        render={<Button variant={triggerVariant} size="sm" />}
      >
        {trigger}
      </AlertDialogTrigger>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>{message}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <Button variant="destructive" disabled={pending} onClick={run}>
            {pending ? <Spinner data-icon="inline-start" /> : null}
            {confirm}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
