import { useState } from "react"
import { createFileRoute, redirect } from "@tanstack/react-router"
import { devSignIn, getAccount, getDevLogin } from "@/lib/account"
import { Header } from "@/components/header"
import { ThemeToggle } from "@/components/theme-toggle"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"

export const Route = createFileRoute("/sign-in")({
  validateSearch: (search): { failed?: 1 } =>
    search.failed ? { failed: 1 } : {},
  loader: async () => {
    if (await getAccount()) throw redirect({ to: "/" })
    return { devLogin: await getDevLogin() }
  },
  component: SignIn,
})

function SignIn() {
  const { devLogin } = Route.useLoaderData()
  const { failed } = Route.useSearch()

  return (
    <div className="flex min-h-svh flex-col">
      <Header>
        <ThemeToggle />
      </Header>
      <main className="mx-auto flex w-full max-w-sm flex-1 flex-col justify-center gap-8 px-6 pt-12 pb-[calc(7rem+10vh)]">
        <div className="flex flex-col gap-2">
          <h1 className="text-2xl font-medium tracking-tight">
            Sign in to Motile
          </h1>
          <p className="text-muted-foreground">
            Manage your servers and clients.
          </p>
        </div>
        <div className="flex flex-col gap-3">
          <Button
            size="lg"
            nativeButton={false}
            render={<a href="/auth/start" />}
          >
            <GoogleIcon />
            Continue with Google
          </Button>
          {failed && (
            <p role="alert" className="text-sm text-destructive">
              The sign-in didn't finish. Try again.
            </p>
          )}
          {devLogin && <DevSignIn />}
        </div>
        <p className="text-sm text-muted-foreground">
          By signing in you agree to the{" "}
          <a
            href="https://motile.app/terms/"
            className="underline underline-offset-4 hover:text-foreground"
          >
            terms
          </a>{" "}
          and the{" "}
          <a
            href="https://motile.app/privacy/"
            className="underline underline-offset-4 hover:text-foreground"
          >
            privacy policy
          </a>
          .
        </p>
      </main>
    </div>
  )
}

function DevSignIn() {
  const [email, setEmail] = useState("dev@motile.app")
  const [error, setError] = useState<string>()

  async function submit(event: React.FormEvent) {
    event.preventDefault()
    try {
      window.location.assign(await devSignIn({ data: email }))
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    }
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-2 border-t pt-3">
      <div className="flex gap-2">
        <Input
          type="email"
          required
          aria-label="Email"
          value={email}
          onChange={(event) => setEmail(event.target.value)}
        />
        <Button type="submit" variant="outline">
          Dev sign-in
        </Button>
      </div>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
    </form>
  )
}

function GoogleIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        fill="currentColor"
        d="M21.35 11.1h-9.17v2.98h5.27c-.23 1.26-.94 2.33-2 3.04v2.5h3.22c1.9-1.75 2.99-4.32 2.99-7.37 0-.39-.1-.78-.31-1.15ZM12.18 22c2.7 0 4.96-.89 6.61-2.42l-3.22-2.5c-.9.6-2.04.95-3.39.95-2.6 0-4.81-1.76-5.6-4.12H3.25v2.58A9.98 9.98 0 0 0 12.18 22ZM6.58 13.91a6 6 0 0 1 0-3.82V7.51H3.25a10 10 0 0 0 0 8.98l3.33-2.58Zm5.6-7.94c1.47 0 2.79.5 3.83 1.5l2.86-2.86A9.6 9.6 0 0 0 12.18 2a9.98 9.98 0 0 0-8.93 5.51l3.33 2.58c.79-2.36 3-4.12 5.6-4.12Z"
      />
    </svg>
  )
}
