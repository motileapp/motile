import { createFileRoute } from "@tanstack/react-router"
import { authUrl, publicUrl, startSignIn } from "@/server/auth"

export const Route = createFileRoute("/auth/start")({
  server: {
    handlers: {
      GET: () => {
        const { state, challenge } = startSignIn()
        const parameters = new URLSearchParams({
          challenge,
          state,
          redirect: `${publicUrl}/auth/callback`,
        })
        return new Response(null, {
          status: 302,
          headers: { location: `${authUrl}/auth/start?${parameters}` },
        })
      },
    },
  },
})
