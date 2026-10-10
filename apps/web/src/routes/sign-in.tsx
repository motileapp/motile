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
        <ThemeToggle className="-mr-2" />
      </Header>
      <main className="mx-auto flex w-full max-w-sm flex-1 flex-col justify-center gap-8 px-4 pt-12 sm:px-6 pb-[calc(7rem+10vh)]">
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
    <svg viewBox="0 0 48 48" aria-hidden="true">
      <path
        className="fill-google-red"
        d="M24 9.5c3.54 0 6.71 1.22 9.21 3.6l6.85-6.85C35.9 2.38 30.47 0 24 0 14.62 0 6.51 5.38 2.56 13.22l7.98 6.19C12.43 13.72 17.74 9.5 24 9.5z"
      />
      <path
        className="fill-google-blue"
        d="M46.98 24.55c0-1.57-.15-3.09-.38-4.55H24v9.02h12.94c-.58 2.96-2.26 5.48-4.78 7.18l7.73 6c4.51-4.18 7.09-10.36 7.09-17.65z"
      />
      <path
        className="fill-google-yellow"
        d="M10.53 28.59c-.48-1.45-.76-2.99-.76-4.59s.27-3.14.76-4.59l-7.98-6.19C.92 16.46 0 20.12 0 24c0 3.88.92 7.54 2.56 10.78l7.97-6.19z"
      />
      <path
        className="fill-google-green"
        d="M24 48c6.48 0 11.93-2.13 15.89-5.81l-7.73-6c-2.15 1.45-4.92 2.3-8.16 2.3-6.26 0-11.57-4.22-13.47-9.91l-7.98 6.19C6.51 42.62 14.62 48 24 48z"
      />
    </svg>
  )
}
