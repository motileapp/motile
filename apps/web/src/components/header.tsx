import { Logo } from "@/components/logo"
import { buttonVariants } from "@/components/ui/button"
import { cn } from "@/lib/utils"

export function Header({ children }: { children: React.ReactNode }) {
  return (
    <header className="mx-auto flex h-16 w-full max-w-6xl items-center justify-between px-4">
      <a
        href="https://motile.app"
        className={cn(
          buttonVariants({ variant: "ghost" }),
          "-ml-3.25 text-base text-foreground"
        )}
      >
        <Logo />
      </a>
      <nav className="flex items-center gap-1 text-muted-foreground">
        {children}
      </nav>
    </header>
  )
}
