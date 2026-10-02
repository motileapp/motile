import { HeadContent, Scripts, createRootRoute } from "@tanstack/react-router"
import { Toaster } from "@/components/ui/sonner"

import appCss from "../styles.css?url"

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: "utf-8" },
      { name: "viewport", content: "width=device-width, initial-scale=1" },
      { title: "Motile" },
      {
        name: "description",
        content: "The hosts and apps on your Motile account.",
      },
    ],
    links: [
      { rel: "stylesheet", href: appCss },
      { rel: "icon", type: "image/svg+xml", href: "/logo.svg" },
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
    <html lang="en">
      <head>
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
