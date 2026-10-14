export const GITHUB_URL = "https://github.com/motileapp/motile"
export const WEB_APP_URL = "https://app.motile.app"
export const TAGLINE = "The command center for coding agents"

const LATEST_RELEASE = `${GITHUB_URL}/releases/latest/download`

/** A system a client is built for. */
export type Platform = "macos" | "linux" | "windows"

export const PLATFORMS: { platform: Platform; name: string }[] = [
  { platform: "macos", name: "macOS" },
  { platform: "linux", name: "Linux" },
  { platform: "windows", name: "Windows" },
]

export type Download = {
  platform: Platform
  /** The processor it is built for, where the system has more than one. */
  arch?: "x86_64" | "arm64"
  title: string
  detail: string
} & (
  | { url: string; command?: never }
  /** Copied, to run in a terminal. */
  | { command: string; url?: never }
)

/** Every client a release carries; the first of a system is the one it is offered. */
export const DOWNLOADS: Download[] = [
  {
    platform: "macos",
    title: "Apple Silicon",
    detail: "M1 or later, on macOS 14 or later",
    url: `${LATEST_RELEASE}/Motile.zip`,
  },
  {
    platform: "linux",
    title: "Copy the install command",
    detail: "Intel, AMD and ARM",
    command: "curl -fsSL https://motile.app/linux | sh",
  },
  {
    platform: "windows",
    arch: "x86_64",
    title: "Intel and AMD",
    detail: "Most Windows PCs",
    url: `${LATEST_RELEASE}/Motile-GPUI-windows-x86_64.zip`,
  },
  {
    platform: "windows",
    arch: "arm64",
    title: "ARM",
    detail: "Snapdragon PCs",
    url: `${LATEST_RELEASE}/Motile-GPUI-windows-aarch64.zip`,
  },
]
