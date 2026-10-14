import { Fragment, useSyncExternalStore } from "react"
import { ChevronDownIcon } from "lucide-react"
import { OSIcon } from "@/components/os-icon"
import { buttonVariants } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { DOWNLOADS, PLATFORMS, type Download } from "@/lib/links"
import { cn } from "@/lib/utils"

/** The download for the system the page is read on, as far as the browser says. */
function offered(): Download {
  const agent = navigator.userAgent
  const platform = navigator.platform
  const arm = /aarch64|arm64|armv8/i.test(`${agent} ${platform}`)
  const of = (system: Download["platform"]) =>
    DOWNLOADS.find(
      (download) =>
        download.platform === system &&
        (!download.arch || (download.arch === "arm64") === arm)
    ) ?? DOWNLOADS.find((download) => download.platform === system)
  if (/Win/.test(platform)) return of("windows") ?? DOWNLOADS[0]
  if (/Linux/.test(platform) && !/Android/.test(agent))
    return of("linux") ?? DOWNLOADS[0]
  return DOWNLOADS[0]
}

/** The system never changes while the page is open. */
const unchanging = () => () => {}

/** Downloads the client for the reader's system, with every other one behind the chevron. */
export function DownloadButton() {
  const primary = useSyncExternalStore(unchanging, offered, () => DOWNLOADS[0])
  const system = PLATFORMS.find(({ platform }) => platform === primary.platform)
  return (
    <div className="flex">
      <a
        href={primary.url}
        data-slot="button"
        className={cn(
          buttonVariants({ size: "lg" }),
          "rounded-r-none border-r-0 bg-clip-border pr-3.5"
        )}
      >
        <OSIcon os={primary.platform} />
        Download for {system?.name}
      </a>
      <DropdownMenu>
        <DropdownMenuTrigger
          aria-label="Every download"
          className={cn(
            buttonVariants({ size: "icon-lg" }),
            "w-9 rounded-l-none border-l-primary-foreground/(--opacity-color-tint-stronger) bg-clip-border aria-expanded:bg-primary/(--opacity-lit)"
          )}
        >
          <ChevronDownIcon />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-64">
          {PLATFORMS.map(({ platform, name }, index) => (
            <Fragment key={platform}>
              {index > 0 && <DropdownMenuSeparator />}
              <DropdownMenuGroup>
                <DropdownMenuLabel>{name}</DropdownMenuLabel>
                {DOWNLOADS.filter(
                  (download) => download.platform === platform
                ).map((download) => (
                  <DropdownMenuItem
                    key={download.url}
                    render={<a href={download.url} />}
                    className="items-start"
                  >
                    <OSIcon os={platform} className="mt-0.5" />
                    <span className="flex flex-col gap-0.5">
                      {download.title}
                      <span className="text-xs font-normal text-muted-foreground">
                        {download.detail}
                      </span>
                    </span>
                  </DropdownMenuItem>
                ))}
              </DropdownMenuGroup>
            </Fragment>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  )
}
