import { createHash, randomBytes } from "node:crypto"
import {
  deleteCookie,
  getCookie,
  setCookie,
} from "@tanstack/react-start/server"

const SESSION_COOKIE = "motile_session"
const SIGN_IN_COOKIE = "motile_sign_in"
const SIGN_IN_SECONDS = 600

export const authUrl = (process.env.AUTH_URL ?? "https://motile.app").replace(
  /\/$/,
  ""
)
export const publicUrl = (
  process.env.PUBLIC_URL ?? "http://localhost:3000"
).replace(/\/$/, "")
export const devLogin = process.env.DEV_LOGIN === "1"

const cookie = {
  httpOnly: true,
  secure: publicUrl.startsWith("https://"),
  sameSite: "lax",
} as const

export function sessionToken() {
  return getCookie(SESSION_COOKIE)
}

export function endSession() {
  deleteCookie(SESSION_COOKIE, { ...cookie, path: "/" })
}

/** Remembers the secret of a sign-in in the browser that starts it, and returns what the auth server is told. */
export function startSignIn() {
  const state = randomBytes(32).toString("hex")
  const verifier = randomBytes(32).toString("hex")
  setCookie(SIGN_IN_COOKIE, `${state}.${verifier}`, {
    ...cookie,
    path: "/auth",
    maxAge: SIGN_IN_SECONDS,
  })
  const challenge = createHash("sha256").update(verifier).digest("hex")
  return { state, challenge }
}

/** Opens a session with the code the auth server sent back. Only works in the browser that started the sign-in. */
export async function finishSignIn(code: string, state: string) {
  const [startedState, verifier] = (getCookie(SIGN_IN_COOKIE) ?? "").split(".")
  deleteCookie(SIGN_IN_COOKIE, { ...cookie, path: "/auth" })
  if (!verifier || state !== startedState) return false

  const response = await fetch(`${authUrl}/api/sessions`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ code, verifier }),
  })
  if (!response.ok) return false
  const session: { token: string; expires_at: number } = await response.json()
  setCookie(SESSION_COOKIE, session.token, {
    ...cookie,
    path: "/",
    expires: new Date(session.expires_at * 1000),
  })
  return true
}

export class SignedOut extends Error {
  constructor() {
    super("You are signed out. Sign in again.")
  }
}

/** Calls the auth server as the person signed in. */
export async function api<T>(method: string, path: string): Promise<T> {
  const token = sessionToken()
  if (!token) throw new SignedOut()

  const response = await fetch(`${authUrl}${path}`, {
    method,
    headers: { authorization: `Bearer ${token}` },
  }).catch(() => undefined)
  if (!response) throw new Error("Motile can't be reached right now.")
  if (response.status === 401) {
    endSession()
    throw new SignedOut()
  }
  const body = await response.json().catch(() => undefined)
  if (!response.ok) throw new Error(body?.error ?? "Something went wrong.")
  return body
}
