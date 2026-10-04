import { createServerFn } from "@tanstack/react-start"
import {
  SignedOut,
  api,
  authUrl,
  devLogin,
  endSession,
  sessionToken,
  startSignIn,
} from "@/server/auth"

export type Device = {
  public_key: string
  kind: "server" | "client"
  name: string
  platform: string
  created_at: number
}

export type Account = {
  user: { email: string; name: string | null; picture: string | null }
  servers: Array<Device>
  clients: Array<Device>
}

export type InstallCommand = { command: string; expires_at: number }

type Me = { user: Account["user"] | null; devices: Array<Device> }

/** The account of the person signed in, or `null`. */
export const getAccount = createServerFn().handler(
  async (): Promise<Account | null> => {
    const me = await api<Me>("GET", "/api/me").catch((error) => {
      if (error instanceof SignedOut) return null
      throw error
    })
    if (!me?.user) return null
    return {
      user: me.user,
      servers: me.devices.filter((device) => device.kind === "server"),
      clients: me.devices.filter((device) => device.kind === "client"),
    }
  }
)

export const createInstallCommand = createServerFn({ method: "POST" }).handler(
  () => api<InstallCommand>("POST", "/api/enroll-tokens")
)

export const removeDevice = createServerFn({ method: "POST" })
  .inputValidator((publicKey: string) => publicKey)
  .handler(async ({ data: publicKey }) => {
    await api("DELETE", `/api/devices/${encodeURIComponent(publicKey)}`)
  })

export const signOut = createServerFn({ method: "POST" }).handler(async () => {
  if (!sessionToken()) return
  await api("DELETE", "/api/sessions/current").catch(() => undefined)
  endSession()
})

export const getDevLogin = createServerFn().handler(() => devLogin)

/** Signs in without Google and returns where the browser finishes the sign-in. Only with `DEV_LOGIN=1`. */
export const devSignIn = createServerFn({ method: "POST" })
  .inputValidator((email: string) => email)
  .handler(async ({ data: email }) => {
    if (!devLogin) throw new Error("Not found")
    const { state, challenge } = startSignIn()
    const response = await fetch(`${authUrl}/api/dev/login`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ challenge, email, web: true }),
    })
    const body = await response.json()
    if (!response.ok) throw new Error(body.error ?? "Something went wrong.")
    return `/auth/callback?${new URLSearchParams({ code: body.code, state })}`
  })
