import { createFileRoute } from "@tanstack/react-router"
import { finishSignIn, publicUrl } from "@/server/auth"

export const Route = createFileRoute("/auth/callback")({
  server: {
    handlers: {
      GET: async ({ request }) => {
        const query = new URL(request.url).searchParams
        const [code, state] = [query.get("code"), query.get("state")]
        const signedIn = code && state && (await finishSignIn(code, state))
        const path = signedIn ? "/" : "/sign-in?failed=1"
        return new Response(null, {
          status: 302,
          headers: { location: `${publicUrl}${path}` },
        })
      },
    },
  },
})
