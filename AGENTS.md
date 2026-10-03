## What is this?

Motile is the open-source command center for coding agents. The agents (Claude Code and Codex)
run on machines the user owns, called servers. Native apps drive them: an app connects straight to
its servers over [iroh](https://www.iroh.computer), which needs no open ports, and keeps a local
copy of every thread so it opens where it was left. There is no local mode; an app always talks
to a server.

These programs make it up, plus the code the apps share:

| Program | Where it runs | What it does |
| --- | --- | --- |
| Auth server (`apps/auth`) | auth.motile.app | Signs people in with Google, records which devices belong to an account |
| Marketing site (`apps/marketing`) | motile.app, as static files | The landing page, privacy and terms, and the installer at `/install.sh` |
| Web app (`apps/web`) | app.motile.app | Lists an account's servers and apps, adds servers, removes devices |
| Server (`apps/server`, the `motile` binary) | The user's Linux machines and Macs | Runs the agents, stores threads in SQLite, serves the account's apps |
| Mac app (`apps/macos`) | The user's Mac | The interface |
| Core (`crates/core`) | Inside every app | Account, connections, sync, the local cache, rendering transcripts |

Every device is an ed25519 key, which is also its iroh address. The auth server only says which
keys belong to one account; it never sees a thread. A server accepts connections only from its
account's apps. The web app is not a device: it holds a session, which can manage the account
but can never connect to a server. README.md covers how a user sets things up.

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

- `wire.rs` is every message between an app and a server. Changing it changes both sides; bump
  `PROTOCOL_VERSION` when an old app or server could no longer understand the other.
- `auth_api.rs` and `auth_client.rs` are the auth server's JSON and the client for it.
- `identity.rs` is the device key and request signing: a linked device signs its requests to the
  auth server instead of holding a token.
- `media.rs` names an image or a video by its contents, so that a server and the app that sent
  it the file call it the same.

### apps/auth (Rust, Axum, sqlx on Postgres)

- `sign_in.rs` is the browser's side of a sign-in: `/auth/start`, Google, and back with a
  one-time code, to the app at `motile://auth` or to the web app at `WEB_URL/auth/callback`.
  `google.rs` is the OIDC exchange.
- `api.rs` is what apps, servers and the web app call: exchanging the code for a linked device or
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
  `public/install.sh` is the installer: it downloads the server from the latest release and runs
  `motile setup`. `scripts/icons.mjs` draws the icons of both web projects from the
  mark: `pnpm --filter motile-marketing icons`.
- `apps/web` runs on its own server. `src/server/auth.ts` holds the session: the browser only
  gets an HttpOnly cookie, and the server calls the auth server with the session's token.
  `src/lib/account.ts` is the server functions the pages call, and `src/routes/auth/` starts and
  finishes a sign-in.

### apps/server

- `hub.rs` is the live state of every thread. A turn is one run of the agent's CLI; its events
  are applied there, saved, and sent to every app that has the thread open. The agent's process
  is talked to over its stdin while it runs: it asks before a tool call that needs approval and
  waits for the app's answer, which is also how its questions to the user are answered and its
  plan is approved. Claude Code is also told when the thread's model, effort or access change;
  Codex takes them when its next process starts. A message sent while a turn runs is queued:
  it waits until the turn ends and starts the next one, and it joins the transcript when the
  agent says it has read it. A queued message can be sent now, which the running turn takes at
  once (Claude Code moves what it runs to the background or stops its reply for it, Codex takes
  it after its next tool call), or taken back, and the ones a stopped turn leaves behind wait
  until they are sent. The queue is only in memory. Claude Code's process stays after a turn while something it started is still
  running (a monitor, a background shell): the thread is then `monitoring`, the process takes
  the next messages itself, and it starts turns of its own when what it watches reports.
  A thread can work in a git worktree of its own instead of the project's folder: its first turn
  makes the worktree in the server's data folder, on a branch that starts from the branch the
  user picked as the remote has it, runs the project's setup script there as a tool call, and
  starts the agent. The branch has a temporary name until the writer has named it from the
  first message. A worktree that has gone is made again on its branch before the next turn,
  and it goes with its thread, while the branch stays.
  In a repository, the folder is kept as it is before a turn's agent starts and when the turn
  ends: a snapshot, which is a commit under a ref of the thread's own that the repository's
  branches and index never see. What the turn changed is the difference between the two, and
  joins the item that ends the turn once it has been read.
- `pacing.rs` says how much of a streamed reply is finished. The hub passes text on in finished
  blocks (a paragraph, a list item, a line of code), not token by token.
- `agents/` builds the command for a turn, the lines its process is given and parses its
  output: `claude.rs` for `claude -p --input-format stream-json --output-format stream-json
  --replay-user-messages --permission-prompt-tool stdio`, `codex.rs` for `codex app-server`,
  which is JSON-RPC: its parser asks for the thread and the turn as the answers arrive. Both
  become the same `AgentEvent`s. Codex presents a plan when its turn has ended and carries it
  out in a turn of its own. `models.rs` lists Claude's models by hand; Codex's are read from
  its cache.
- `store.rs` is SQLite. Every item has a position (`seq`) and the revision that last changed it
  (`rev`); an app asks for what changed after the revision it has.
- `title.rs` generates thread titles with the thread's own agent.
- `setup.rs` is what the installer runs: it checks for agents, links the server with the install
  token, and installs the service. `service.rs` is that service: on Linux a systemd system unit
  that runs as the installing user (for root it sets `IS_SANDBOX=1`, without which Claude Code
  refuses full access), on a Mac a launchd agent in the user's desktop session, where the
  agents find the Keychain, the simulators and code signing. The installer puts the binary in
  `/usr/local/bin` on Linux and in `~/.local/bin` on a Mac, where it can update itself without
  a password.
- `access.rs` asks the auth server which apps belong to the account, and caches the answer.
- `icons.rs` finds a project's icon in its folder (a favicon, icon or logo file, also in the
  `apps` and `packages` of a workspace). The path is kept with the project; the user can pick
  another image instead.
- `git.rs` lists the branches of a project's folder and switches or creates one there, with the
  `git` program. The project's folder has one branch for all the threads that work in it; it
  refuses while an agent is working there. It also makes the worktrees of the threads that
  work in one of their own.
  It also reads what isn't committed or pushed in the folder a thread works in, and commits,
  pulls, pushes and opens a pull request there when an app asks; pull requests are GitHub's,
  through `gh`. The status is read when a turn ends and when an app asks, never on a timer, and
  goes to the apps with the project. What git refuses reaches the user in git's words.
  It takes the snapshots of the threads, and answers with the patch of a turn, of what isn't
  committed, or of everything since the branch left the one it started from.
- `files.rs` lists the server's folders for choosing a project, takes the attachments an app
  sends, and lists and reads the files of the folder a thread works in, never outside it.
- `github.rs` is the server's GitHub login, through `gh`: whether it is there, the repositories
  it reaches and cloning one. A project started from a name (a folder with `git init`) or
  cloned from GitHub goes in `~/projects` on the server.
- `drafts.rs` has an agent write the commit message or the pull request's text from the changes,
  for the user to edit before it is used, and name branches the way the server's instructions
  say, which the user can change in the app's settings and put back. `generate.rs` is how it and `title.rs` ask an agent's
  CLI for a short answer as JSON.
- `media.rs` keeps the images and videos agents show. An agent shows one by writing a Markdown
  image that points at a file on the server; the agents are told so when they start. The server
  copies the file then, named by its contents, and the item says what it shows and how large it
  is. So a thread shows the same thing after the file has changed or gone. An app asks for the
  bytes by that name, and the copy goes when the last thread that shows it is deleted. The
  images and videos attached to a user's message are kept the same way, each video with the
  poster the app made of it.
- `files.rs` takes the files an app uploads, each into a folder of its own in `attachments`. An
  app uploads a file when it is attached, before the message is sent. The files go with the
  thread their message is in, and one that was never sent goes after a day.
- `update.rs` replaces the server's own program with the latest release's for this OS and CPU
  and starts it again, when an app asks. It refuses while an agent is working.
- `tests/e2e.rs` runs the server against `scripts/fake-agent` over real iroh connections.

### crates/core

Built as a static library for the apps (`ffi.rs`: JSON commands in, JSON events out) and as a
Rust library for tests.

- `core.rs` is one loop that owns all state. Commands from the app and events from the servers
  arrive on one channel; anything that waits on the network runs in its own task.
- `link.rs` keeps one server connected and follows its thread list and the open threads.
- `cache.rs` is the app's SQLite copy of its servers' threads. A thread is read from it a page
  at a time: whole turns, about 150 items. An open thread holds its last turns, the ones
  before them come when the app scrolls near the first row, and the app has the turns far
  above let go again while it shows the end. So a thread of any length costs what is looked at.
- `media.rs` is the images and videos the app has fetched from its servers, as files. They are
  fetched when a row that shows one is seen, and take at most 2 GB: past that, what was looked
  at longest ago goes first. The servers keep them all, so the app can also clear them. An
  image or a video the app uploads is kept here too, so it shows without being fetched back.
- `git.rs` says which git action a project's status calls for: commit, pull, push or a pull
  request. The apps show that one.
- `render/diff.rs` reads a patch into files whose lines the apps draw, and highlights them a
  line at a time, as it does a whole file's.
- `browse.rs` is browsing a server's folders by typing a path: the folders of the directory
  typed so far, narrowed by what follows its last slash.
- `render/` turns transcripts into rows ready to draw: `markdown.rs` (text with style runs, in
  UTF-16 offsets), `highlight.rs` (syntect; streaming code is highlighted incrementally) and
  `rows.rs` (the row list and the splices sent to the app; tool calls that follow one another
  are one row, a finished turn's work folds behind one, as does what the agent did before a
  message it took mid-turn, an image or a video is a row that knows its size before the file
  is there, a message of the user names the images and videos attached to it, the files a turn
  changed are a row before the turn's end, under their folders, and the messages that wait for
  the agent are the last rows, each saying how it waits).
- `api.rs` is the JSON the app and the core exchange. `examples/drive.rs` drives the core from a
  terminal, and `examples/seed.rs` signs a data folder in with the dev login and makes the dev
  app's project and threads.

### apps/macos (Swift: SwiftUI, with AppKit for the transcript)

- `Core/CoreBridge.swift` calls the Rust core; `Core/AppStore.swift` is all the state the views
  show. Events are decoded off the main thread.
- `App/MotileApp.swift` lays out the window: the sidebar and the thread side by side on one
  surface, with a line between them. It is not a `NavigationSplitView`, whose sidebar the system
  draws as a floating panel.
- `Views/Transcript/` is the transcript: `TranscriptView.swift` only keeps views for the rows on
  screen, `RowViews.swift` are the rows, `Rows.swift` builds their text and measures their
  height off the main thread, so a row's height is known before it is scrolled to. The view
  asks the core for the turns before its first row when it is scrolled near them.
  `MediaRowView.swift` is the row of an image or a video: images are decoded off the main
  thread at the size they are shown, and a video is downloaded when it is played. A queued
  message is a row under the line that says the agent is working, with the buttons that send
  it now or take it back. `AttachedFilesView.swift` is the files in a message's bubble: tiles
  of one size for the images and videos, and the names of the others.
- `Views/Sidebar`, `Views/Thread`, `Views/Composer` and `Views/Onboarding` are SwiftUI.
  `Views/Composer/ComposerStrips.swift` is the strips against the composer's top and bottom:
  that the agent is monitoring, and the server, folder and branch the thread works in, with the
  branch picker. A new thread chooses there between the project's folder and a new worktree,
  and then picks the branch the worktree starts from.
  `Views/Composer/AttachmentViews.swift` is the attached files above the text: a tile for an
  image or a video and a chip for any other file, each saying how far its upload is. A message
  can't be sent until its files are on the server.
  `Views/MediaViewer.swift` shows the images and videos of a message or of the composer one at
  a time over the whole window, when one is clicked. Its `ZoomingScrollView` is how an image
  zooms there and in the panel.
  `Views/Thread/GitControl.swift` is the git button in the top bar of a thread and its popover:
  the files and the message of a commit, or the title and text of a pull request, to change
  before they are used.
  `Views/CommandPanel.swift` is the panel behind ⌘K, ⌘N and ⌘P. A project is added there too:
  a new one from a name, one of the user's GitHub repositories, or a folder of the server,
  browsed by typing its path. Rows that wait for a server are placeholders of the same size.
  `Views/Shared` holds the window's glass surface, the hover highlight and the agents' and
  projects' icons.
- `Views/Panel` is the panel on the right of the thread, with tabs kept for each thread: the
  changes in the folder the thread works in, its files, and the files opened from either.
  `CodeView.swift` draws a diff or a file, and only the lines on screen. `Core/SidePanel.swift`
  is the panel's state. A turn's changed files in the transcript open its diff there. The
  panel can be maximized for a thread: it then covers the thread, which keeps its width behind
  it, and the window shows the sidebar and the panel.
- `Core/AppUpdater.swift` updates the app itself: it downloads the release's app, checks that
  it is signed by the developer who signed the running one, puts it in its place and restarts.
  A copy that isn't signed with the Developer ID can't update itself.
- `Resources/AppIcon.icon` is the app icon, made in Icon Composer. `scripts/build-app.sh`
  compiles it with `actool`, which also draws the flat icon older macOS versions show.
- `Demo/DemoDriver.swift` walks the app through a scripted demo; `scripts/ci-demo.sh` runs it
  in CI against a real auth server and a real server and collects screenshots and `checks.txt`.
- `scripts/dev-app.sh` opens the dev app, `scripts/shot.sh` takes a picture of its window and
  `scripts/click.sh` clicks in it. See Development.

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
- A server runs agents with full access to its machine. It must only ever accept devices of its
  own account.
- A machine that runs agents is a server, in the code and in what the user reads. What the user
  reads says "your server" or its name, not "the server" alone, which sounds like ours. The auth
  server is always called the auth server.
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
    cargo run -p motile-server -- run           # a server, once linked with `motile setup <token>`
    cargo run -p motile-core --example drive    # the core, driven from a terminal
    apps/macos/scripts/dev-app.sh               # on a Mac: the app, on an account of its own
    apps/macos/scripts/build-app.sh --open      # on a Mac: the app, to sign in with Google

Checks (`cargo test` needs the compose Postgres; it creates a throwaway database per test):

    export DATABASE_URL=postgres://motile:motile@localhost:5435/motile
    cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
    pnpm -r lint && pnpm -r typecheck && pnpm -r build    # after changing either web project

The Mac app can't be built on Linux. The `macOS` workflow builds it and the server for Macs on
every push that touches them, runs the demo and uploads the app, the server and the screenshots:

    gh run watch                                      # then
    gh run download --name screenshots

To see a UI change on a Mac, use the dev app, from `apps/macos`:

    scripts/dev-app.sh             # builds what changed and opens the app; seconds after the first run
    scripts/shot.sh shot.png       # a picture of its window
    scripts/click.sh 85 330        # a click, counted in points from the window's top left corner

The dev app is signed in as `demo@motile.app` on an auth server with the dev login, and its
server, `studio`, runs `scripts/fake-agent` and starts with a project and three finished
threads. All of it lives in `apps/macos/build/dev` and keeps running between runs (Postgres on
5436, the auth server on 3112, the server on 47614), so sessions in the same tree share it.
`dev-app.sh` restarts only what was rebuilt, and `dev-app.sh --stop` stops everything. It never
touches a real account. Reach other states by sending the fake agent's prompts below. Type with
`osascript` (System Events `keystroke`), and address the app by the pid in `build/dev/app.pid`.
A picture taken on a Retina display has two pixels per point. `osascript` needs Accessibility
and the program that runs the agent needs Screen Recording, both granted once in System
Settings. Use the demo (`scripts/ci-demo.sh`) only for the stall numbers.

`scripts/fake-agent` stands in for Claude Code and Codex in the tests and the demo. It replays
the recorded output in `fixtures/` or makes up a turn, depending on the prompt. Asked to
"watch the deploy" it stays after its turn, as Claude Code does while it monitors, asked to
"run greet.py" under supervised access it asks before each tool call, and asked to "show the
screenshot" it makes an image and shows it, and asked to "greet by name" it really changes
files in its folder. "Which color" has it
ask the user a question, and "plan the hello" present a plan to approve. A message sent now
while it works is read after its next tool call, as both agents do, or stops a reply that
streams, as Claude Code does; "run greet.py" ends its reply with it. Asked for a commit
message, a pull request's text or a branch's name, the way it is asked for a title, it writes
one.

## Releasing and deploying

To release, set `version` in `Cargo.toml` to the new version, commit, and push the tag
`v<version>`; the workflow refuses a tag that doesn't match. It publishes the server and the auth
server for Linux, and the server and the app for Macs, as a GitHub release. The installer and the download button
always fetch the latest release, and apps and servers compare their own version with it to offer
an update. The app in a release is signed with the Developer ID certificate and
notarized, using the repository's `APPLE_*` secrets; running the `macOS` workflow by hand with
`sign` does the same without a release.

Unbind builds the `Marketing`, `Web` and `Auth` services from `main` and deploys them on every
push that touches their files.

## Commit Messages

Commit messages start with the part of the system they touched, followed by a short imperative
sentence describing the change:

    auth: Refuse a sign-in code that was started by another app
    macos: Show the server's round-trip time in the sidebar
    server | core | macos: Stream replies in finished blocks

The parts are the folders in `apps` and `crates`: `auth`, `marketing`, `web`, `server`, `macos`,
`core` and `protocol`. Use `ci` for the workflows and `docs` for README.md and AGENTS.md.

The title should be concise. Description should explain the work in more detail (only if
required) while still being concise. Use simple language, do not try to sound smart.
