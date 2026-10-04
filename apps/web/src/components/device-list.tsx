import { useState } from "react"
import { useRouter } from "@tanstack/react-router"
import { LaptopIcon, ServerIcon } from "lucide-react"
import { toast } from "sonner"
import type { Device } from "@/lib/account"
import { removeDevice } from "@/lib/account"
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
              {PLATFORMS[device.platform] ?? device.platform} · Added{" "}
              {DAY.format(device.created_at * 1000)}
            </ItemDescription>
          </ItemContent>
          <ItemActions>
            <RemoveDevice device={device} />
          </ItemActions>
        </Item>
      ))}
    </ItemGroup>
  )
}

function RemoveDevice({ device }: { device: Device }) {
  const router = useRouter()
  const [open, setOpen] = useState(false)
  const [removing, setRemoving] = useState(false)

  async function remove() {
    setRemoving(true)
    try {
      await removeDevice({ data: device.public_key })
      toast.success(`${device.name} was removed.`)
      setOpen(false)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error))
    }
    setRemoving(false)
    await router.invalidate()
  }

  return (
    <AlertDialog open={open} onOpenChange={setOpen}>
      <AlertDialogTrigger render={<Button variant="ghost" size="sm" />}>
        Remove
      </AlertDialogTrigger>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Remove {device.name}?</AlertDialogTitle>
          <AlertDialogDescription>
            {device.kind === "server"
              ? "Your clients will no longer reach this server. Its threads stay on the machine, and it can be added again with a new install command."
              : "This client is signed out and can no longer reach your servers."}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <Button variant="destructive" disabled={removing} onClick={remove}>
            Remove
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
