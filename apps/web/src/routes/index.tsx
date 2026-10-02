import { createFileRoute, redirect } from "@tanstack/react-router"
import { LaptopIcon, ServerIcon } from "lucide-react"
import { getAccount } from "@/lib/account"
import { cn } from "@/lib/utils"
import { AccountMenu } from "@/components/account-menu"
import { AddServer } from "@/components/add-server"
import { DeviceList } from "@/components/device-list"
import { Logo } from "@/components/logo"
import { buttonVariants } from "@/components/ui/button"
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty"

export const Route = createFileRoute("/")({
  loader: async () => {
    const account = await getAccount()
    if (!account) throw redirect({ to: "/sign-in" })
    return account
  },
  component: Account,
})

function Account() {
  const account = Route.useLoaderData()

  return (
    <div className="mx-auto w-full max-w-2xl px-4 pb-16 sm:px-6">
      <header className="flex h-16 items-center justify-between">
        <a href="https://motile.app">
          <Logo />
        </a>
        <AccountMenu user={account.user} />
      </header>
      <main className="flex flex-col gap-12 pt-8">
        <section className="flex flex-col gap-4">
          <div className="flex items-end justify-between gap-4">
            <Heading
              title="Servers"
              description="The machines your agents run on."
            />
            <AddServer servers={account.servers} />
          </div>
          {account.servers.length > 0 ? (
            <DeviceList devices={account.servers} />
          ) : (
            <Empty className="border">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <ServerIcon />
                </EmptyMedia>
                <EmptyTitle>No servers yet</EmptyTitle>
                <EmptyDescription>
                  Add a Linux machine or a Mac and your apps can start threads on it.
                </EmptyDescription>
              </EmptyHeader>
            </Empty>
          )}
        </section>
        <section className="flex flex-col gap-4">
          <Heading
            title="Apps"
            description="The devices signed in to your account. They can reach every server."
          />
          {account.apps.length > 0 ? (
            <DeviceList devices={account.apps} />
          ) : (
            <Empty className="border">
              <EmptyHeader>
                <EmptyMedia variant="icon">
                  <LaptopIcon />
                </EmptyMedia>
                <EmptyTitle>No apps yet</EmptyTitle>
                <EmptyDescription>
                  Sign in to Motile on your Mac and it shows up here.
                </EmptyDescription>
              </EmptyHeader>
              <EmptyContent>
                <a
                  href="https://github.com/motileapp/motile/releases/latest/download/Motile.zip"
                  className={cn(buttonVariants({ variant: "outline" }))}
                >
                  Download for macOS
                </a>
              </EmptyContent>
            </Empty>
          )}
        </section>
      </main>
    </div>
  )
}

function Heading({
  title,
  description,
}: {
  title: string
  description: string
}) {
  return (
    <div className="flex flex-col gap-1">
      <h2 className="text-lg font-medium tracking-tight">{title}</h2>
      <p className="text-sm text-muted-foreground">{description}</p>
    </div>
  )
}
