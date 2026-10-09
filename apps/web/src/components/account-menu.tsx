import { useRouter } from "@tanstack/react-router"
import { LogOutIcon } from "lucide-react"
import type { Account } from "@/lib/account"
import { signOut } from "@/lib/account"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

export function AccountMenu({ user }: { user: Account["user"] }) {
  const router = useRouter()

  async function leave() {
    await signOut()
    await router.invalidate()
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label="Account"
        className="rounded-full outline-none focus-visible:ring-3 focus-visible:ring-tint-ring"
      >
        <Avatar>
          {user.picture && (
            <AvatarImage src={user.picture} referrerPolicy="no-referrer" />
          )}
          <AvatarFallback>
            {(user.name ?? user.email).slice(0, 1).toUpperCase()}
          </AvatarFallback>
        </Avatar>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-56">
        <DropdownMenuGroup>
          <DropdownMenuLabel className="flex flex-col">
            {user.name && (
              <span className="text-sm font-medium text-foreground">
                {user.name}
              </span>
            )}
            {user.email}
          </DropdownMenuLabel>
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={leave}>
          <LogOutIcon />
          Sign out
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
