import { HeadContent, Scripts, createRootRoute } from "@tanstack/react-router"
import { Toaster } from "@/components/ui/sonner"
import { THEME_SCRIPT } from "@/lib/theme"

import appCss from "../styles.css?url"

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: "utf-8" },
      { name: "viewport", content: "width=device-width, initial-scale=1" },
      { title: "Motile" },
      {
        name: "description",
        content: "The servers and clients on your Motile account.",
      },
      { property: "og:title", content: "Motile" },
      { property: "og:image", content: "https://app.motile.app/preview.png" },
      { property: "og:image:width", content: "1200" },
      { property: "og:image:height", content: "630" },
      { name: "twitter:card", content: "summary_large_image" },
    ],
    links: [
      { rel: "stylesheet", href: appCss },
      { rel: "icon", href: "/favicon.ico", sizes: "48x48" },
      { rel: "icon", type: "image/svg+xml", href: "/favicon.svg" },
      { rel: "apple-touch-icon", href: "/apple-touch-icon.png" },
      { rel: "manifest", href: "/site.webmanifest" },
    ],
  }),
  notFoundComponent: NotFound,
  shellComponent: RootDocument,
})

function NotFound() {
  return (
    <main className="mx-auto flex min-h-svh max-w-sm flex-col justify-center gap-2 px-6">
      <h1 className="text-lg font-medium">Not found</h1>
      <p className="text-muted-foreground">
        There is nothing at this address.{" "}
        <a href="/" className="text-foreground underline underline-offset-4">
          Go to your account
        </a>
      </p>
    </main>
  )
}

function RootDocument({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className="dark" suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: THEME_SCRIPT }} />
        <HeadContent />
      </head>
      <body>
        {children}
        <Toaster position="bottom-center" />
        <Scripts />
      </body>
    </html>
  )
}
