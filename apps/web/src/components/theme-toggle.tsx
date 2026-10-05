import { MoonIcon, SunIcon } from "lucide-react"
import { toggleTheme } from "@/lib/theme"
import { Button } from "@/components/ui/button"

export function ThemeToggle() {
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      aria-label="Switch between dark and light"
      onClick={toggleTheme}
    >
      <SunIcon className="hidden dark:block" />
      <MoonIcon className="dark:hidden" />
    </Button>
  )
}
