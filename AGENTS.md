## What is this?

Motile is the open-source command center for coding agents. The agents (Claude Code and Codex)
run on machines the user owns, called servers. Native clients drive them: a client connects straight to
its servers over [iroh](https://www.iroh.computer), which needs no open ports, and keeps a local
copy of every thread so it opens where it was left. There is no local mode; a client always talks
to a server.

These programs make it up, plus the code the clients share:

| Program | Where it runs | What it does |
| --- | --- | --- |
| Auth server (`apps/auth`) | auth.motile.app | Signs people in with Google, records which devices belong to an account |
| Marketing site (`apps/marketing`) | motile.app, as static files | The landing page, privacy and terms, and the installer at `/install.sh` |
| Web app (`apps/web`) | app.motile.app | Lists an account's servers and clients, adds servers, removes devices |
| Server (`apps/server`, the `motile` binary) | The user's Linux machines and Macs | Runs the agents, stores threads in SQLite, serves the account's clients |
| Mac app (`apps/macos`) | The user's Mac | The interface |
| iOS app (`apps/ios`) | The user's iPhone and iPad | The interface |
| Apple kit (`packages/apple`) | Inside the Mac and the iOS app | Their state and their views |
| GPUI app (`apps/gpui`) | The user's Mac | The Mac app again, in Rust with GPUI. A trial: no workflow builds it |
| Core (`crates/core`) | Inside every client | Account, connections, sync, the local cache, rendering transcripts |

Every device is an ed25519 key, which is also its iroh address. The auth server only says which
keys belong to one account; it never sees a thread. A server accepts connections only from its
account's clients. The web app is not a device: it holds a session, which can manage the account
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

What the programs agree on.

- `wire.rs`: every message between a client and a server. Bump `PROTOCOL_VERSION` when an old
  client or server could no longer understand the other.
- `auth_api.rs`, `auth_client.rs`: the auth server's JSON and the client for it.
- `identity.rs`: the device key and request signing.
- `media.rs`: names an image or a video by its contents.

### apps/auth (Rust, Axum, sqlx on Postgres)

- `sign_in.rs`: the browser's side of a sign-in. `google.rs` is the OIDC exchange.
- `api.rs`: what clients, servers and the web app call. A caller is a device that signed the
  request or a session's `Bearer` token.
- `config.rs`: builds the install command a token comes with.
- `pages.rs`: the page a failed sign-in ends on, drawn like the web app's with the colours of
  `packages/theme/tokens.css` and DM Sans from `assets/`.
- `migrations/`: the schema.
- `e2e/`: the real router on a fresh database per test, with a fake Google. `e2e/whole.rs`
  runs all three programs together.
- `DEV_LOGIN=1` lets anyone sign in as anyone without Google. It is for tests, the demo and
  local work, and must never be set in production.

### apps/marketing (Astro, static) and apps/web (TanStack Start)

One pnpm workspace. Both use shadcn/ui (preset `b1VlIvUO`); add components with
`pnpm dlx shadcn@latest add <name>` inside the app.

- Both take their colours from `packages/theme/theme.css`, the Mac app's `Theme.swift` in CSS.
  Dark is the default; the theme switch keeps the choice in a cookie on motile.app, which both
  read.
- `apps/marketing` ships no JavaScript but the theme switch and the demo: `src/components/demo`
  is the Mac app's window in React, with made-up threads. `public/install.sh` is the installer.
  `scripts/icons.mjs` draws the icons of both web projects: `pnpm --filter motile-marketing icons`.
  `scripts/preview.mjs` draws the link preview image of both from the `/preview/` page, the
  tagline over the demo: `pnpm --filter motile-marketing preview-image`. Run it when the demo
  changes.
- `apps/web`: `src/server/auth.ts` holds the session (the browser only gets an HttpOnly
  cookie), `src/lib/account.ts` is the server functions the pages call, and `src/routes/auth/`
  starts and finishes a sign-in.

### apps/server

- `hub.rs`: the live state of every thread: turns, queued messages, approvals, monitoring,
  worktrees, snapshots and the agents an agent starts.
- `pacing.rs`: passes a streamed reply on in finished blocks.
- `agents/`: builds the command for a turn and parses its output into `AgentEvent`s
  (`claude.rs`, `codex.rs`). `models.rs` lists Claude's models by hand.
- `store.rs`: SQLite. Every item has a position (`seq`) and the revision that last changed it
  (`rev`); a client asks for what changed after the revision it has. It also keeps what the
  agents spend, by model and by what it was spent on (a turn, or writing a title, a branch's
  name, a commit message or a pull request): Claude Code reports a session's total with each
  result and Codex what each answer took.
- `pricing.rs`: what the models cost at the API's prices, fetched from LiteLLM's list once a day,
  to say what the tokens Codex spent would have cost. Claude Code says what its own cost.
- `git.rs`: branches, worktrees, status, commit, pull, push, pull requests (through `gh`),
  snapshots and patches.
- `pull_requests.rs`: what GitHub says of a pull request (one `gh api graphql` read), the
  repository's list of them, and what the pull request tab does to one: merge, auto-merge, update
  its branch, close, revert, review its lines, edit, react. `hub.rs` watches a thread's pull
  request for its agent and settles the thread once it merges.
- `files.rs`: browses the server's folders, takes uploads, and reads the files of a thread's
  folder, never outside it.
- `media.rs`: keeps the images and videos threads show, named by their contents.
- `title.rs`, `drafts.rs`: have an agent write titles, commit messages, pull request texts and
  branch names, through `generate.rs`, which also says what each answer took so that it is kept
  with the usage. A title is told what the pull requests and issues its message links to are
  about.
- `github.rs`: the server's GitHub login, its repositories and cloning one.
- `linear.rs`: the server's connections to Linear workspaces. The user approves Motile's
  application in a browser and the server exchanges the code with the verifier it made (PKCE, no
  secret), so the token never leaves the server, which renews it. With it the server reads a
  workspace's teams, users and issues, searches them, reads one with its comments, changes its
  status, assignee and priority, comments on it and files new ones. `MOTILE_LINEAR_URL` stands
  in for Linear.
- `icons.rs`: finds a project's icon in its folder.
- `access.rs`: asks the auth server which clients belong to the account.
- `setup.rs`, `service.rs`: what the installer runs, and the systemd or launchd service.
- `update.rs`: replaces the server's own program with the latest release.
- `tests/e2e.rs`: runs the server against `scripts/fake-agent` over real iroh connections.

### crates/core

A static library for the clients (`ffi.rs`: JSON commands in, JSON events out) and a Rust library
for tests.

- `core.rs`: one loop that owns all state. Anything that waits on the network runs in its own
  task.
- `link.rs`: keeps one server connected and follows its threads.
- `follow.rs`: keeps the cached copy of an active thread that isn't open current.
- `cache.rs`: the client's SQLite copy of its servers' threads, read a page of whole turns at a
  time.
- `media.rs`: the images and videos the client has fetched, as files, with a size limit.
- `git.rs`: which git action a project's status calls for.
- `pull_request.rs`: the pull request tabs: a pull request's statuses, the button its state calls
  for, its menu, its activity and conversations with the Markdown set, the list's rows, and the
  prompts that hand conflicts, failures and lines to the agent.
- `linear.rs`: the Linear tabs: issues under their statuses, as Linear lists them, one issue
  with its description and comments set, and the prompt that hands it to an agent.
- `browse.rs`: browsing a server's folders by typing a path.
- `usage.rs`: what the agents spent on every server, added up for the usage view: a series for
  each agent, and the models, projects and kinds of token.
- `render/`: turns transcripts into rows ready to draw: `rows.rs` (the row list and its
  splices), `markdown.rs`, `highlight.rs`, `diff.rs`, `agents.rs`.
- `api.rs`: the JSON the client and the core exchange.
- `examples/drive.rs` drives the core from a terminal; `examples/seed.rs` makes the dev app's
  account, project and threads.

### packages/apple (Swift: SwiftUI, with AppKit and UIKit for the transcript)

`MotileKit`, the Swift package both clients are made of. `Sources/MotileKit/Shared` is what both
use, `Mac` and `iOS` what only one does, each file of those inside `#if os(…)`. A view that has
to be AppKit on the Mac and UIKit on iOS has a twin in each, named alike (`KitMac.swift`,
`KitIOS.swift`); what the two do is written once, in `Shared`, on top of them.

- `Shared/Platform/Platform.swift`: what the two systems call differently, behind one name.
  Sizes are written as they are on the Mac; `Platform.scale` enlarges them on iOS.
- `Shared/Platform/Symbol.swift`: the icons, drawn from Lucide's font (`Fonts/lucide.ttf`), not
  SF Symbols. A new one is a case with the character it has in lucide-static's
  `font/codepoints.json`. Linear's logo, which Lucide doesn't have, is drawn by hand there.
- `Shared/Core`: `CoreBridge.swift` calls the Rust core, `AppStore.swift` is the state the
  views show, `SidePanel.swift` the side panel's state, `Linear.swift` the Linear tab's.
- `Shared/Views/Transcript`: the transcript. `TranscriptView.swift` only keeps views for the
  rows on screen, `Rows.swift` builds their text and measures them off the main thread,
  `RowViews.swift` are the rows.
- `Shared/Views/Sidebar`, `Composer`, `Onboarding`, `Thread`: SwiftUI.
- `Shared/Views/UI`: the components every view is made of. `ActionButton` and `ActionMenu` (a
  variant, one of three sizes, an optional symbol, a pending state), `Spinner`, `Chip`,
  `InputField`. Their sizes are `ControlSize` and their corners `Radius`, in `Theme.swift`.
- `Shared/Views/CommandPanel.swift`: the panel behind ⌘K, ⌘N and ⌘P, a sheet on iOS.
- `Shared/Views/Settings`: the settings, a route over the whole client with a way back: the
  sections and a search of their groups in a sidebar (`SettingsSidebar.swift`), the open
  section's page (`SettingsPage.swift`), and `UsageView.swift`, the Usage section: a chart of
  cost or tokens by agent, and the models, projects and kinds of token under it. The Mac lays
  them out in `Mac/SettingsRoute.swift`, iOS in `iOS/SettingsScreen.swift`.
- `Shared/Views/Panel`: the panel on the right of the thread: changes, files, agents and pull
  requests (`PullRequestView.swift`, `PullRequestActivity.swift`, `PullRequestList.swift`,
  `LineCommentSheet.swift`, `PullRequestParts.swift`) and Linear (`LinearView.swift`).
  `CodeView.swift` draws a diff or a file.
- `Mac/`: `MotileApp.swift` lays out the window, `MediaViewer.swift`, `AppUpdater.swift`,
  `Glass.swift`, `Demo/DemoDriver.swift`.
- `iOS/`: `MotileAppIOS.swift` is the client and its layout, `Drawer.swift` the sidebar under the
  thread, `PanelScreen.swift`, `ThreadScreen.swift`, `SidebarScreen.swift`, `Tables.swift`,
  `DemoDriverIOS.swift` the steps `do.sh` runs.

### apps/macos and apps/ios

- `apps/macos`: the Mac app's executable and what builds it (`scripts/build-app.sh`).
  `Resources/AppIcon.icon` is the icon of both clients. `scripts/ci-demo.sh` runs the demo in CI.
- `apps/ios`: the Xcode project, for iOS 18 and later. `scripts/build-core.sh` builds the core
  for iOS, `MotileUITests` uses the client with fingers, `scripts/testflight.sh` uploads a build.
- The dev app scripts of both are under Development.

### apps/gpui (Rust: GPUI with GPUI Kit)

Paused: see the rule under General Rules.

The Mac app ported to Rust. A Cargo workspace of its own with its own `Cargo.lock`, so the
checks and the workflows leave it alone; run `cargo` from `apps/gpui`. It uses `crates/core` as
a Rust library, and its data folder is `Motile GPUI`, so it is a device of its own beside the
Mac app.

- `bridge.rs`: runs the core and prepares a transcript's rows off the main thread. `store/` is
  the state the views show, as `AppStore.swift` is, a module per area (`threads.rs`, `git.rs`,
  `projects.rs`, `pull_requests.rs`, `linear.rs`, `settings.rs`, `sidebar.rs`).
- `root.rs`, `main_view.rs`, `app_menu.rs`: the window, its layout, the menu bar and shortcuts.
  `settings/` is the settings route over the window, with `usage.rs` the Usage chart.
- `theme.rs`: `Theme.swift` in Rust: the colours, the `Surface` ladder, `ControlSize` and
  `Radius`. The typeface is DM Sans, bundled in `assets/fonts`.
- `ui/`: the component library of `Shared/Views/UI` (`control.rs`: `ActionButton`,
  `ActionMenu`, `Spinner`, `Chip`, `Switch`, `Segmented`, `InputField`, cards), the menus the
  app draws itself (`menu.rs`), alerts and sheets, the Lucide icons by the names of
  `Symbol.swift` (`icons.rs`). Nothing native is imitated: a menu, an alert or a sheet is drawn
  in the app's own style.
- `transcript/`: `model.rs` (the rows and their splices into GPUI's list), `prose.rs`,
  `rows.rs`, `view.rs`.
- `composer/`, `sidebar.rs`, `thread/`, `git.rs`, `command_panel.rs`, `media/`,
  `onboarding.rs`, `folder_picker.rs`: the views of the same names in the Apple kit.
- `panel/`: the side panel: `state.rs` (the tabs, as `SidePanel.swift`), `view.rs` (the chrome
  and the changes, files and agents tabs), `code_view.rs`, `pull_request.rs`,
  `pull_request_list.rs`, `line_comment.rs` and `linear.rs`.
- `scripts/dev-app.sh` opens its dev app on a stack of its own, with `--mac` the Mac app beside
  it to compare. `scripts/build-app.sh` builds `Motile GPUI.app`.

## General Rules:

- Keep it simple. Do not overcomplicate things.
- The UI must never stall. Nothing slow runs on the main thread: parsing, highlighting, decoding
  and text layout preparation happen in the core or on a background queue, and the transcript
  only ever builds what is on screen. If a change touches how the transcript builds, lays out
  or scrolls its rows, run the macOS workflow and read the stall numbers in `checks.txt`.
- The auth server is security-critical. The sign-in code is only ever sent to `motile://auth` or
  to the web app's callback, works once, and only with the secret that started the sign-in. A
  code sent to the web app only opens a session and never links a device. Codes and tokens are
  stored hashed. Anything that changes this needs a test in `apps/auth/src/e2e`.
- A server runs agents with full access to its machine. It must only ever accept devices of its
  own account.
- A machine that runs agents is a server, in the code and in what the user reads. What the user
  reads says "your server" or its name, not "the server" alone, which sounds like ours. The auth
  server is always called the auth server.
- A program that drives servers is a client, in the code and in what the user reads. "App" is
  only said with what it is: the Mac app, the iOS app, the web app (which is not a client), the
  dev app.
- Rendering logic belongs in `crates/core`, not in a client, so that every future client (Android,
  Windows, Linux) gets it.
- The marketing site's demo (`apps/marketing/src/components/demo`) shows the Mac app as it is.
  When a change alters anything it shows (a view, a colour, a label, a state), update the demo in
  the same change.
- The Mac app and the iOS app do the same things. What one gets, the other gets in the same
  change, and what both do is written once, in `packages/apple/Sources/MotileKit/Shared`. Only
  what a system does differently is written twice.
- A button, a menu, a spinner, a chip or a field in the clients comes from
  `packages/apple/Sources/MotileKit/Shared/Views/UI`. Its size is a `ControlSize`, never a
  number, and nothing is smaller than `.small`. A view does not style a control by hand; what is
  missing is added to the component. A button inside something is as far from its top and bottom
  as from its side. A button that waits is `pending`: it shows the spinner, and a spinner is
  never followed by "…".
- Ignore `apps/gpui` for now. We are not working on it currently: do not read it, change it or
  keep it in step with the Mac app unless we ask for it.
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
- Do not open a dev app or a simulator, or take screenshots or videos, unless we ask for it or
  the change is to layout or behaviour you can't judge from the code. A colour, a token, a
  string, a font size or a padding is edited and reported, nothing more.
- The app's screenshots and videos you show us should be in dark mode unless we say otherwise.
- If we are missing a glaring issue when we ask you to do something, do not hesitate to
  point it out.
- Never commit or push code unless explicitly asked to do so.
- Never make a PR unless explicitly asked to do so.
- Do not insert yourself into our code, commits or PRs in any way. Our codebase is not your
  ad space.
- When a deploy changes required env vars, deploy the code first and change the variables
  after. Unbind restarts the running pod on every variable change.
- Do not slow the machine to a crawl. Other sessions work on it at the same time. Before you
  start a dev app, a simulator or a big build, look at what already runs and how much memory is
  free (`memory_pressure`, `xcrun simctl list devices booted`, `pgrep -fl build/dev`). If there
  is headroom, go ahead. If not, wait for it instead of adding to the pile. Use one simulator at
  a time, and when you are done, stop what you started: `scripts/dev-app.sh --stop`, which on iOS
  also shuts its simulator down.
- After you make code changes, run the checks below and fix what they raise.

## Development

Needs Rust stable, Docker (for Postgres), and Node 24 with pnpm for the marketing site and the
web app. The Mac app and the iOS app need a Mac with Xcode 26 or later, the iOS app also
`rustup target add aarch64-apple-ios aarch64-apple-ios-sim`.

    docker compose up -d                        # Postgres on localhost:5435
    cargo run -p motile-auth                    # with the variables from .env.example exported
    pnpm install
    pnpm --filter motile-marketing dev          # the marketing site on localhost:4321
    pnpm --filter motile-web dev                # the web app on localhost:3001
    cargo run -p motile-server -- run           # a server, once linked with `motile setup <token>`
    cargo run -p motile-core --example drive    # the core, driven from a terminal
    apps/macos/scripts/build-app.sh --open      # the Mac app, to sign in with Google
    open apps/ios/Motile.xcodeproj              # the iOS app, to run on a device from Xcode

Checks (`cargo test` needs the compose Postgres; it creates a throwaway database per test):

    export DATABASE_URL=postgres://motile:motile@localhost:5435/motile
    cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace
    pnpm -r lint && pnpm -r typecheck && pnpm -r build    # after changing either web project
    packages/apple/scripts/check-ui.sh                    # after changing the clients' views

The `Release` workflow runs the demo on every push to `main` that touches the client or the server:

    gh run watch                                      # then
    gh run download --name screenshots

### The dev apps

When we ask to see a UI change, or a layout change can't be judged from the code, use the dev
app. A change to a view both clients share is then looked at in both.

    # from apps/macos
    scripts/dev-app.sh             # builds what changed and opens the client
    scripts/dev-app.sh --stop      # stops everything
    scripts/shot.sh shot.png       # a picture of its window, two pixels per point on Retina
    scripts/click.sh 85 330        # a click, in points from the window's top left corner

    # from apps/ios, in the simulator; works with the Mac's screen locked
    scripts/dev-app.sh                      # builds what changed and opens the client
    scripts/shot.sh shot.png                # a picture of its screen
    scripts/do.sh "sidebar open"            # has the client do something; the steps are in DemoDriverIOS.swift
    scripts/do.sh "type run greet.py" send
    scripts/ui-test.sh                      # the tests that swipe and tap (MotileUITests)
    MOTILE_SIM="iPad Pro 13-inch (M5)" scripts/dev-app.sh    # another simulator

Both are signed in as `demo@motile.app` on an auth server with the dev login, never a real
account. Their server, `studio`, runs `scripts/fake-agent` and starts with a project and three
finished threads. All of it lives in `apps/macos/build/dev` and keeps running until you stop it,
which you do when your task is done; its ports are in `build/dev/ports`. On the Mac, type with `osascript` (System Events
`keystroke`, which needs Accessibility; pictures need Screen Recording) and address the client by
the pid in `build/dev/app.pid`. Use the demo (`scripts/ci-demo.sh`) only for the stall numbers.

### The fake agent

`scripts/fake-agent` stands in for Claude Code and Codex in the tests, the demo and the dev
clients. It replays the recorded output in `fixtures/` or makes up a turn, depending on the prompt:

| Prompt | What it does |
| --- | --- |
| "watch the deploy" | Stays after its turn, as Claude Code does while it monitors |
| "run greet.py" | Under supervised access, asks before each tool call |
| "show the screenshot" | Makes an image and shows it |
| "greet by name" | Really changes files in its folder |
| "ask two agents" (Claude Code), "ask an agent" (Codex) | Starts agents that say what they do and report |
| "which color" | Asks the user a question |
| "plan the hello" | Presents a plan to approve |

A message sent now while it works is read after its next tool call, or stops a reply that
streams (Claude Code). Asked for a title, a commit message, a pull request's text or a branch's
name, it writes one.

`scripts/fake-gh` stands in for GitHub's `gh` in the tests and the dev apps (through
`MOTILE_GH_PATH`), whose project has a local `origin` to push to. It keeps its pull requests in
`build/dev/github.json`; edit that file to see a pull request fail its checks, conflict, wait for
a review, have conversations on its lines or sit in a stack, and set `delay` to see a client wait.

`scripts/fake-linear` stands in for Linear in the dev apps (through `MOTILE_LINEAR_URL`), whose
server starts out connected to its workspace. It keeps its team, users and issues in
`build/dev/linear.json`. `MOTILE_DEV_LINEAR=real scripts/dev-app.sh`, after a `--stop`, leaves
Linear the real one, to connect a workspace of your own.

## Releasing and deploying

Every push to `main` that touches the server, the auth server or the Mac app is a release, for
now. The `Release` workflow publishes the server and the auth server for Linux, and the server and
the client for Macs, as a GitHub release. Its version is the first two numbers of `version` in
`Cargo.toml` and the number of commits as the third (`scripts/release-version.sh`), so set
`version` only to change the first two. The installer and the download button always fetch the
latest release, and clients and servers compare their own version with it to offer an update. The
client in a release is signed with the Developer ID certificate and notarized, using the repository's
`APPLE_*` secrets; running the `macOS` workflow by hand with `sign` does the same without a
release.

Every push to `main` that touches the iOS app uploads it to TestFlight
(`apps/ios/scripts/testflight.sh`, with the repository's `APPLE_TEAM_ID` and `IOS_BUNDLE_ID`
variables), with the version in `Cargo.toml` as it is. It is signed with the Apple Distribution
certificate and the App Store profile in the `IOS_*` secrets, which expire on 2027-10-04.

Unbind builds the `Marketing`, `Web` and `Auth` services from `main` and deploys them on every
push that touches their files.

## Commit Messages

Commit messages start with the part of the system they touched, followed by a short imperative
sentence describing the change:

    auth: Refuse a sign-in code that was started by another client
    macos: Show the server's round-trip time in the sidebar
    server | core | macos: Stream replies in finished blocks

The parts are the folders in `apps`, `crates` and `packages`: `auth`, `marketing`, `web`, `server`,
`macos`, `ios`, `apple`, `gpui`, `core` and `protocol`. Use `ci` for the workflows and `docs` for README.md and AGENTS.md.

The title should be concise. Description should explain the work in more detail (only if
required) while still being concise. Use simple language, do not try to sound smart.
