## What is this?

Motile is the open-source command center for coding agents. The agents (Claude Code and Codex)
run on machines the user owns, called hosts. Native apps drive them: an app connects straight to
its hosts over [iroh](https://www.iroh.computer), which needs no open ports, and keeps a local
copy of every thread so it opens where it was left. There is no local mode; an app always talks
to a host.

These programs make it up, plus the code the apps share:

| Program | Where it runs | What it does |
| --- | --- | --- |
| Auth server (`apps/auth`) | auth.motile.app | Signs people in with Google, records which devices belong to an account |
| Marketing site (`apps/marketing`) | motile.app, as static files | The landing page, privacy and terms, and the installer at `/install.sh` |
| Web app (`apps/web`) | app.motile.app | Lists an account's hosts and apps, adds hosts, removes devices |
| Host (`apps/server`, the `motile` binary) | The user's Linux machines | Runs the agents, stores threads in SQLite, serves the account's apps |
| Mac app (`apps/macos`) | The user's Mac | The interface |
| Core (`crates/core`) | Inside every app | Account, connections, sync, the local cache, rendering transcripts |

Every device is an ed25519 key, which is also its iroh address. The auth server only says which
keys belong to one account; it never sees a thread. A host accepts connections only from its
account's apps. The web app is not a device: it holds a session, which can manage the account
but can never connect to a host. README.md covers how a user sets things up.

Production is the `Motile` project on Unbind:

| Service | Address | What it runs |
| --- | --- | --- |
| `Marketing` | https://motile.app | The marketing site, built from `apps/marketing/Dockerfile` |
| `Web` | https://app.motile.app | The web app, built from `apps/web/Dockerfile` |
| `Auth` | https://auth.motile.app | The auth server, built from `Dockerfile` |
| `Postgres` | | The auth server's database |

## Repo Structure:

### crates/protocol

What the three programs agree on.

- `wire.rs` is every message between an app and a host. Changing it changes both sides; bump
  `PROTOCOL_VERSION` when an old app or host could no longer understand the other.
- `auth_api.rs` and `auth_client.rs` are the auth server's JSON and the client for it.
- `identity.rs` is the device key and request signing: a linked device signs its requests to the
  auth server instead of holding a token.

### apps/auth (Rust, Axum, sqlx on Postgres)

- `sign_in.rs` is the browser's side of a sign-in: `/auth/start`, Google, and back with a
  one-time code, to the app at `motile://auth` or to the web app at `WEB_URL/auth/callback`.
  `google.rs` is the OIDC exchange.
- `api.rs` is what apps, hosts and the web app call: exchanging the code for a linked device or
  for a web session (`/api/sessions`), install tokens (`/api/enroll-tokens`, `/api/enroll`),
  `/api/me`, removing devices. A caller is a device that signed the request or a session's
  `Bearer` token.
- `config.rs` builds the install command a token comes with. It runs the installer at
  `INSTALL_URL`, and names this auth server in it unless it is Motile's own.
- `pages.rs` is the plain page a sign-in that failed ends on.
- `migrations/` is the schema. Expired sign-ins and tokens are deleted every minute.
- `e2e/` runs the real router on a fresh database per test, with a fake Google. `e2e/whole.rs`
  runs all three programs together.
- `DEV_LOGIN=1` lets anyone sign in as anyone without Google. It exists for tests, the Mac demo
  and local work, and must never be set in production.

### apps/marketing (Astro, static) and apps/web (TanStack Start)

Both use shadcn/ui (preset `b1VlIvUO`: Base UI, neutral colors, Tailwind 4) and follow the
system's light or dark appearance. They are one pnpm workspace; add components with
`pnpm dlx shadcn@latest add <name>` inside the app.

- `apps/marketing` builds to `dist`, which static-web-server serves in production
  (`server.toml`). It ships no JavaScript; React only renders at build time.
  `public/install.sh` is the installer: it downloads the host from the latest release and runs
  `motile setup`. `scripts/icons.mjs` draws the icons of both web projects from the
  mark: `pnpm --filter motile-marketing icons`.
- `apps/web` runs on its own server. `src/server/auth.ts` holds the session: the browser only
  gets an HttpOnly cookie, and the server calls the auth server with the session's token.
  `src/lib/account.ts` is the server functions the pages call, and `src/routes/auth/` starts and
  finishes a sign-in.

### apps/server (the host)

- `hub.rs` is the live state of every thread. A turn is one run of the agent's CLI; its events
  are applied there, saved, and sent to every app that has the thread open. Claude Code's
  process is talked to over its stdin while it runs: it asks before a tool call that needs
  approval and waits for the app's answer, which is also how its questions to the user are
  answered and its plan is approved, and it is told when the thread's model, effort or access
  change. It stays after a turn while something it started is still running (a monitor,
  a background shell): the thread is then `monitoring`, the process takes the next messages
  itself, and it starts turns of its own when what it watches reports.
- `pacing.rs` says how much of a streamed reply is finished. The hub passes text on in finished
  blocks (a paragraph, a list item, a line of code), not token by token.
- `agents/` builds the command for a turn and parses its output: `claude.rs` for
  `claude -p --input-format stream-json --output-format stream-json --permission-prompt-tool
  stdio`, `codex.rs` for `codex exec --json`. Both become the same `AgentEvent`s. `models.rs` lists Claude's models by hand; Codex's are read from its cache.
- `store.rs` is SQLite. Every item has a position (`seq`) and the revision that last changed it
  (`rev`); an app asks for what changed after the revision it has.
- `title.rs` generates thread titles with the thread's own agent.
- `setup.rs` is what the installer runs: it checks for agents, links the host with the install
  token, and installs the systemd unit. The unit is a system unit that runs as the installing
  user; for root it sets `IS_SANDBOX=1`, without which Claude Code refuses full access.
- `access.rs` asks the auth server which apps belong to the account, and caches the answer.
- `icons.rs` finds a project's icon in its folder (a favicon, icon or logo file, also in the
  `apps` and `packages` of a workspace). The path is kept with the project; the user can pick
  another image instead.
- `media.rs` keeps the images and videos agents show. An agent shows one by writing a Markdown
  image that points at a file on the host; the agents are told so when they start. The host
  copies the file then, named by its contents, and the item says what it shows and how large it
  is. So a thread shows the same thing after the file has changed or gone. An app asks for the
  bytes by that name, and the copy goes when the last thread that shows it is deleted.
- `update.rs` replaces the host's own program with the latest release's and starts it again,
  when an app asks. It refuses while an agent is working.
- `tests/e2e.rs` runs the host against `scripts/fake-agent` over real iroh connections.

### crates/core

Built as a static library for the apps (`ffi.rs`: JSON commands in, JSON events out) and as a
Rust library for tests.

- `core.rs` is one loop that owns all state. Commands from the app and events from the hosts
  arrive on one channel; anything that waits on the network runs in its own task.
- `link.rs` keeps one host connected and follows its thread list and the open threads.
- `cache.rs` is the app's SQLite copy of its hosts' threads.
- `media.rs` is the images and videos the app has fetched from its hosts, as files. They are
  fetched when a row that shows one is seen, and take at most 2 GB: past that, what was looked
  at longest ago goes first. The hosts keep them all, so the app can also clear them.
- `render/` turns transcripts into rows ready to draw: `markdown.rs` (text with style runs, in
  UTF-16 offsets), `highlight.rs` (syntect; streaming code is highlighted incrementally) and
  `rows.rs` (the row list and the splices sent to the app; tool calls that follow one another
  are one row, a finished turn's work folds behind one, and an image or a video is a row that
  knows its size before the file is there).
- `api.rs` is the JSON the app and the core exchange. `examples/drive.rs` drives the core from a
  terminal.

### apps/macos (Swift: SwiftUI, with AppKit for the transcript)

- `Core/CoreBridge.swift` calls the Rust core; `Core/AppStore.swift` is all the state the views
  show. Events are decoded off the main thread.
- `App/MotileApp.swift` lays out the window: the sidebar and the thread side by side on one
  surface, with a line between them. It is not a `NavigationSplitView`, whose sidebar the system
  draws as a floating panel.
- `Views/Transcript/` is the transcript: `TranscriptView.swift` only keeps views for the rows on
  screen, `RowViews.swift` are the rows, `Rows.swift` builds their text off the main thread.
  `MediaRowView.swift` is the row of an image or a video: images are decoded off the main
  thread at the size they are shown, and a video is downloaded when it is played.
- `Views/Sidebar`, `Views/Thread`, `Views/Composer` and `Views/Onboarding` are SwiftUI.
  `Views/CommandPanel.swift` is the panel behind ⌘K, ⌘N and ⌘P, and `Views/Shared` holds the
  window's glass surface, the hover highlight and the agents' and projects' icons.
- `Core/AppUpdater.swift` updates the app itself: it downloads the release's app, checks that
  it is signed by the developer who signed the running one, puts it in its place and restarts.
  A copy that isn't signed with the Developer ID can't update itself.
- `Resources/AppIcon.icon` is the app icon, made in Icon Composer. `scripts/build-app.sh`
  compiles it with `actool`, which also draws the flat icon older macOS versions show.
- `Demo/DemoDriver.swift` walks the app through a scripted demo; `scripts/ci-demo.sh` runs it
  in CI against a real auth server and host and collects screenshots and `checks.txt`.

## General Rules:

- Keep it simple. Do not overcomplicate things.
- The UI must never stall. Nothing slow runs on the main thread: parsing, highlighting, decoding
  and text layout preparation happen in the core or on a background queue, and the transcript
  only ever builds what is on screen. If a change touches the transcript, run the macOS workflow
  and read the stall numbers in `checks.txt`.
- The auth server is security-critical. The sign-in code is only ever sent to `motile://auth` or
  to the web app's callback, works once, and only with the secret that started the sign-in. A
  code sent to the web app only opens a session and never links a device. Codes and tokens are
  stored hashed. Anything that changes this needs a test in `apps/auth/src/e2e`.
- A host runs agents with full access to its machine. It must only ever accept devices of its
  own account.
- Rendering logic belongs in `crates/core`, not in an app, so that every future app (iOS,
  Android, Windows, Linux) gets it.
- Do not leave paragraphs of comments on top of the code. You should try to avoid them as much
  as possible with understandable function names and code. If they are necessary even then, make
  them concise. Remove such comments when you come by them in the codebase. Comments should
  always move with code, not be left behind.
- Use guard statement patterns in any code you write.
- Do not edit generated code: `Cargo.lock`, `pnpm-lock.yaml`, `apps/web/src/routeTree.gen.ts`
  and the shadcn components in `src/components/ui`. Never edit an applied migration in
  `apps/auth/migrations` or `apps/server/migrations`, add a new one.
- Do not write useless tests; tests should cover input/output behaviour.
- Reinvent the wheel but do not reinvent the car. If you are solving a simple problem do not
  introduce a library. If you are solving a complex but common problem, there is likely a
  modern library for it, if so, use it.
- Do not start editing code in response to a question. We'll tell you when to edit code.
- If we are missing a glaring issue when we ask you to do something, do not hesitate to
  point it out.
- Never commit or push code unless explicitly asked to do so.
- Never make a PR unless explicitly asked to do so.
- Do not insert yourself into our code, commits or PRs in any way. Our codebase is not your
  ad space.
- When a deploy changes required env vars, deploy the code first and change the variables
  after. Unbind restarts the running pod on every variable change.
- After you make code changes, run the checks below and fix what they raise.

## Development

Needs Rust stable, Docker (for Postgres), and Node 24 with pnpm for the marketing site and the
web app. The Mac app needs Xcode 26 or later.

    docker compose up -d                        # Postgres on localhost:5435
    cargo run -p motile-auth                    # with the variables from .env.example exported
    pnpm install
    pnpm --filter motile-marketing dev          # the marketing site on localhost:4321
    pnpm --filter motile-web dev                # the web app on localhost:3001
    cargo run -p motile-server -- run           # a host, once linked with `motile setup <token>`
    cargo run -p motile-core --example drive    # the core, driven from a terminal
    apps/macos/scripts/build-app.sh --open      # on a Mac

Checks (`cargo test` needs the compose Postgres; it creates a throwaway database per test):

    export DATABASE_URL=postgres://motile:motile@localhost:5435/motile
    cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
    pnpm -r lint && pnpm -r typecheck && pnpm -r build    # after changing either web project

The Mac app can't be built on Linux. The `macOS` workflow builds it on every push that touches it,
runs the demo and uploads the app and the screenshots:

    gh run watch                                      # then
    gh run download --name screenshots

`scripts/fake-agent` stands in for Claude Code and Codex in the tests and the demo. It replays
the recorded output in `fixtures/` or makes up a turn, depending on the prompt. Asked to
"watch the deploy" it stays after its turn, as Claude Code does while it monitors, asked to
"run greet.py" under supervised access it asks before each tool call, and asked to "show the
screenshot" it makes an image and shows it. "Which color" has it
ask the user a question, and "plan the hello" present a plan to approve.

## Releasing and deploying

To release, set `version` in `Cargo.toml` to the new version, commit, and push the tag
`v<version>`; the workflow refuses a tag that doesn't match. It publishes the host and the auth
server for Linux and the Mac app as a GitHub release. The installer and the download button
always fetch the latest release, and apps and hosts compare their own version with it to offer
an update. The app in a release is signed with the Developer ID certificate and
notarized, using the repository's `APPLE_*` secrets; running the `macOS` workflow by hand with
`sign` does the same without a release.

Unbind builds the `Marketing`, `Web` and `Auth` services from `main` and deploys them on every
push that touches their files.

## Commit Messages

Commit messages start with the part of the system they touched, followed by a short imperative
sentence describing the change:

    auth: Refuse a sign-in code that was started by another app
    macos: Show the host's round-trip time in the sidebar
    server | core | macos: Stream replies in finished blocks

The parts are the folders in `apps` and `crates`: `auth`, `marketing`, `web`, `server`, `macos`,
`core` and `protocol`. Use `ci` for the workflows and `docs` for README.md and AGENTS.md.

The title should be concise. Description should explain the work in more detail (only if
required) while still being concise. Use simple language, do not try to sound smart.
