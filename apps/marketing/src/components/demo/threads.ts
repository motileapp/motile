export type Agent = "claude" | "codex"

export type Project = { name: string; color: string }

export type Server = { name: string; path: string; ms: number }

export type Tool = "search" | "read" | "edit" | "run" | "check"

export type Item =
  | { kind: "user"; text: string }
  | { kind: "text"; text: string }
  | { kind: "code"; lang: string; code: string }
  | {
      kind: "tool"
      tool: Tool
      verb: string
      target: string
      running?: boolean
    }
  | { kind: "changes" }
  | { kind: "end"; worked: string }

export type Status =
  | { kind: "idle"; ago: string }
  | { kind: "working"; since: number }
  | { kind: "approval" }
  | { kind: "monitoring"; since: number }

export type Thread = {
  id: string
  title: string
  project: Project
  branch: string
  agent: Agent
  model: string
  server: Server
  status: Status
  pullRequest?: { number: number; title: string }
  worktree?: boolean
  items: Item[]
  approval?: { verb: string; target: string; allowed: Item[]; denied: Item[] }
  diff: string
}

export type DoneThread = {
  title: string
  project: Project
  branch: string
  agent: Agent
  model: string
  server: Server
  ago: string
  pullRequest?: { number: number; title: string }
}

const api: Project = { name: "api", color: "var(--chart-1)" }
const web: Project = { name: "web", color: "var(--chart-2)" }
const mobile: Project = { name: "mobile", color: "var(--chart-3)" }

export const servers: Server[] = [
  { name: "studio", path: "direct", ms: 41 },
  { name: "build-box", path: "direct", ms: 63 },
]

const [studio, buildBox] = servers

export const threads: Thread[] = [
  {
    id: "rate-limit",
    title: "Rate limit the public API",
    project: api,
    branch: "rate-limit",
    agent: "claude",
    model: "Opus 5.5 · Work",
    server: studio,
    status: { kind: "idle", ago: "2m" },
    pullRequest: { number: 128, title: "Rate limit the public API per key" },
    items: [
      {
        kind: "user",
        text: "Add rate limiting to the public API: 60 requests a minute per key. Clients should know when to retry.",
      },
      {
        kind: "tool",
        tool: "search",
        verb: "Searched for",
        target: "app.use(",
      },
      { kind: "tool", tool: "read", verb: "Read", target: "src/server.ts" },
      {
        kind: "text",
        text: "Each API key gets a bucket of 60 requests that refills one a second. A request that finds it empty gets a `429` with `Retry-After`, so a client knows exactly when to come back.",
      },
      { kind: "tool", tool: "edit", verb: "Edited", target: "src/limiter.ts" },
      { kind: "tool", tool: "edit", verb: "Edited", target: "src/server.ts" },
      { kind: "tool", tool: "run", verb: "Ran", target: "pnpm test" },
      {
        kind: "code",
        lang: "ts",
        code: `export function rateLimit(limiter: Limiter) {
  return async (request: Request, next: Next) => {
    const wait = limiter.take(request.apiKey)
    if (wait === null) return next(request)

    return new Response("Too many requests", {
      status: 429,
      headers: { "retry-after": String(Math.ceil(wait / 1000)) },
    })
  }
}`,
      },
      {
        kind: "text",
        text: "### What changed\n- **The limiter** keeps one bucket per key, in memory.\n- **The middleware** runs after auth, so it limits keys, not addresses.\n- All 41 tests pass, four of them new.",
      },
      { kind: "changes" },
      { kind: "end", worked: "Worked for 1m 12s" },
    ],
    diff: `src/limiter.ts new
@@ -0,0 +1,22 @@
+const CAPACITY = 60
+const REFILL_PER_MS = 1 / 1000
+
+type Bucket = { tokens: number; at: number }
+
+export class Limiter {
+  private buckets = new Map<string, Bucket>()
+
+  take(key: string, now = Date.now()) {
+    const bucket = this.buckets.get(key) ?? { tokens: CAPACITY, at: now }
+    bucket.tokens = Math.min(CAPACITY, bucket.tokens + (now - bucket.at) * REFILL_PER_MS)
+    bucket.at = now
+    this.buckets.set(key, bucket)
+
+    if (bucket.tokens >= 1) {
+      bucket.tokens -= 1
+      return null
+    }
+    return (1 - bucket.tokens) / REFILL_PER_MS
+  }
+}
+
src/server.ts
@@ -4,12 +4,15 @@
 import { auth } from "./auth"
 import { routes } from "./routes"
+import { Limiter } from "./limiter"
+import { rateLimit } from "./middleware"

 export function createServer() {
   const app = new App()
+  const limiter = new Limiter()

   app.use(auth())
-  app.use(logRequests())
+  app.use(rateLimit(limiter))
+  app.use(logRequests())
   app.route(routes)

   return app
src/middleware.ts
@@ -18,3 +18,14 @@
   }
 }
+
+export function rateLimit(limiter: Limiter) {
+  return async (request: Request, next: Next) => {
+    const wait = limiter.take(request.apiKey)
+    if (wait === null) return next(request)
+
+    return new Response("Too many requests", {
+      status: 429,
+      headers: { "retry-after": String(Math.ceil(wait / 1000)) },
+    })
+  }
+}`,
  },
  {
    id: "flaky-test",
    title: "Fix the flaky checkout test",
    project: web,
    branch: "fix-checkout-flake",
    agent: "codex",
    model: "6.1-Sol",
    server: buildBox,
    status: { kind: "working", since: 42 },
    worktree: true,
    items: [
      {
        kind: "user",
        text: "checkout.spec.ts fails about one run in five on CI. Find out why and fix it.",
      },
      {
        kind: "tool",
        tool: "run",
        verb: "Ran",
        target: "pnpm test checkout --repeat 20",
      },
      {
        kind: "text",
        text: "It failed 4 of 20 runs, always on the total. The test reads the cart total as soon as the page loads, but the price request can still be on its way, so now and then it sees `$0.00`.",
      },
      {
        kind: "tool",
        tool: "read",
        verb: "Read",
        target: "tests/checkout.spec.ts",
      },
      {
        kind: "tool",
        tool: "edit",
        verb: "Edited",
        target: "tests/checkout.spec.ts",
      },
      {
        kind: "text",
        text: "The test now waits for the total to show a price before it reads it. Running it 50 times to be sure.",
      },
      {
        kind: "tool",
        tool: "run",
        verb: "Running",
        target: "pnpm test checkout --repeat 50",
        running: true,
      },
    ],
    diff: `tests/checkout.spec.ts
@@ -21,9 +21,10 @@
   await page.goto("/cart")
   await page.getByRole("button", { name: "Checkout" }).click()

-  const total = await page.getByTestId("total").textContent()
-  expect(total).toBe("$48.00")
+  const total = page.getByTestId("total")
+  await expect(total).not.toHaveText("$0.00")
+  await expect(total).toHaveText("$48.00")

   await page.getByLabel("Card number").fill(CARD)
   await page.getByRole("button", { name: "Pay" }).click()`,
  },
  {
    id: "settings",
    title: "Move settings to SQLite",
    project: mobile,
    branch: "settings-sqlite",
    agent: "claude",
    model: "Opus 5.5 · Personal",
    server: studio,
    status: { kind: "approval" },
    items: [
      {
        kind: "user",
        text: "Move the app's settings from the plist into the SQLite store, and carry over what users already have.",
      },
      {
        kind: "tool",
        tool: "read",
        verb: "Read",
        target: "Sources/Settings.swift",
      },
      {
        kind: "tool",
        tool: "edit",
        verb: "Edited",
        target: "Sources/SettingsStore.swift",
      },
      {
        kind: "tool",
        tool: "run",
        verb: "Ran",
        target: "swift test --filter Settings",
      },
      {
        kind: "text",
        text: "Settings now live in a `settings` table, and the first launch copies the old plist into it. The tests pass. The old fixture is all that is left.",
      },
    ],
    approval: {
      verb: "Run",
      target: "rm Fixtures/Settings.plist",
      allowed: [
        {
          kind: "tool",
          tool: "run",
          verb: "Ran",
          target: "rm Fixtures/Settings.plist",
        },
        {
          kind: "text",
          text: "Removed it. The migration test still covers a user who has the plist, with a copy it writes itself.",
        },
        { kind: "end", worked: "Worked for 3m 4s" },
      ],
      denied: [
        {
          kind: "text",
          text: "Left the fixture where it is. Everything else is done.",
        },
        { kind: "end", worked: "Worked for 2m 51s" },
      ],
    },
    diff: `Sources/SettingsStore.swift
@@ -1,14 +1,21 @@
 import Foundation
+import SQLite

 final class SettingsStore {
-    private let defaults = UserDefaults.standard
+    private let db: Connection
+
+    init(db: Connection) throws {
+        self.db = db
+        try db.run("CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value BLOB)")
+        try Migration.copyPlist(into: db)
+    }

     func value(for key: String) -> Data? {
-        defaults.data(forKey: key)
+        try? db.scalar("SELECT value FROM settings WHERE key = ?", key) as? Data
     }

     func set(_ value: Data, for key: String) {
-        defaults.set(value, forKey: key)
+        try? db.run("INSERT OR REPLACE INTO settings VALUES (?, ?)", key, value)
     }
 }`,
  },
  {
    id: "deploy",
    title: "Watch the staging deploy",
    project: api,
    branch: "main",
    agent: "claude",
    model: "Sonnet 5.5 · Personal",
    server: studio,
    status: { kind: "monitoring", since: 754 },
    items: [
      {
        kind: "user",
        text: "Deploy main to staging and keep an eye on it until the health checks are green.",
      },
      {
        kind: "tool",
        tool: "run",
        verb: "Ran",
        target: "./scripts/deploy.sh staging",
      },
      {
        kind: "text",
        text: "Deployed `a41c9e2` to staging. Two of the three instances are healthy; the third is still warming its cache.",
      },
      {
        kind: "tool",
        tool: "check",
        verb: "Checked",
        target: "staging.motile.dev/health",
      },
      {
        kind: "text",
        text: "All three answer in under 80 ms now. I'll keep watching for errors and tell you if anything changes.",
      },
      { kind: "end", worked: "Worked for 4m 30s" },
    ],
    diff: "",
  },
]

export const doneThreads: DoneThread[] = [
  {
    title: "Add dark mode to settings",
    project: web,
    branch: "settings-dark-mode",
    agent: "claude",
    model: "Opus 5.5 · Work",
    server: studio,
    ago: "1h",
    pullRequest: { number: 124, title: "Add dark mode to the settings page" },
  },
  {
    title: "Bump iroh to 0.95",
    project: api,
    branch: "bump-iroh",
    agent: "codex",
    model: "6.1-Sol",
    server: buildBox,
    ago: "3h",
    pullRequest: { number: 125, title: "Bump iroh to 0.95" },
  },
  {
    title: "Fix the sidebar flicker",
    project: mobile,
    branch: "sidebar-flicker",
    agent: "claude",
    model: "Sonnet 5.5 · Personal",
    server: studio,
    ago: "5h",
  },
  {
    title: "Write the release notes",
    project: web,
    branch: "main",
    agent: "codex",
    model: "6.1-Sol",
    server: studio,
    ago: "1d",
  },
  {
    title: "Cache the avatar images",
    project: mobile,
    branch: "avatar-cache",
    agent: "claude",
    model: "Opus 5.5 · Personal",
    server: buildBox,
    ago: "2d",
    pullRequest: {
      number: 119,
      title: "Cache the avatar images on disk and evict the oldest past 50 MB",
    },
  },
]
