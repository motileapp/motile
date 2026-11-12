## What is this?

Motile is the open-source command center for coding agents. The agents (Claude Code and Codex)
run on machines the user owns, called servers. Native clients connect straight to their servers
over [iroh](https://www.iroh.computer) and keep a local copy of every thread. There is no local
mode; a client always talks to a server.

| Program | Where it runs | What it does |
| --- | --- | --- |
| Auth server (`apps/auth`) | auth.motile.app | Signs people in with Google, records which devices belong to an account |
| Marketing site (`apps/marketing`) | motile.app, as static files | The landing page, privacy and terms, and the installer at `/i` |
| Web app (`apps/web`) | app.motile.app | Lists an account's servers and clients, adds servers, removes devices |
| Server (`apps/server`, the `motile` binary) | The user's Linux machines and Macs | Runs the agents, stores threads in SQLite, serves the account's clients |
| Mac app (`apps/macos`) | The user's Mac | The interface |
| iOS app (`apps/ios`) | The user's iPhone and iPad | The interface |
| Apple kit (`packages/apple`) | Inside the Mac and the iOS app | Their state and their views |
| GPUI app (`apps/gpui`) | The user's Linux machines and Windows PCs | The Mac app again, in Rust with GPUI |
| Android app (`apps/android`) | The user's Android phones and tablets | The iOS app again, in Kotlin with Jetpack Compose |
| Core (`crates/core`) | Inside every client | Account, connections, sync, the local cache, rendering transcripts |
| Gateway (`apps/gateway`, the `motile-gateway` image) | A machine of the user's that is always on, in Docker | Holds the account's Claude Code and Codex sign-ins and its MCP servers' logins, and passes the agents' requests on with them |

Every device is an ed25519 key, which is also its iroh address. The auth server only says which
keys belong to one account; it never sees a thread. A server accepts connections only from its
account's clients. An account has at most one gateway, which serves only its account. The web app
is not a device: it holds a session, which can manage the account but never connect to a server.

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

- `wire.rs`: every message between a client and a server, and the change logs servers copy each
  other's threads with. Bump `PROTOCOL_VERSION` when an old client or server could no longer
  understand the other.
- `auth_api.rs`, `auth_client.rs`: the auth server's JSON and the client for it.
- `identity.rs`: the device key and request signing.
- `gateway_wire.rs`: what servers and clients say to the gateway.
- `activity.rs`: the features the Activity tab counts.
- `search.rs`: how a search through the messages matches, alike on servers and clients.
- `select.rs`: which account a new thread starts on.
- `media.rs`, `skills.rs`: naming an agent's files by their contents; `$name` skills in a message.

### crates/openui

OpenUI Lang 1.0, the language of the programs agents show in their replies, for the server and
the core alike.

- `lexer.rs` … `serialize.rs`, `js.rs`: the language, streamed. `runtime.rs` is what a host drives,
  `fences.rs` finds a reply's programs.
- `motile.rs`, `components.rs`, `functions/`: Motile's library, open-intelligent-ui's.
  `prompt.rs` writes the prompt the agents are taught with.
- `LAYOUT.md`: every component's geometry and behaviour. Every client follows it.
- `fixtures/`: the spec's conformance cases, open-intelligent-ui's library and prompts, and the
  prompts this crate writes (`OPENUI_BLESS=1` rewrites them). The programs the clients and the
  fake agent show are the repo's `fixtures/openui`.

### apps/auth (Rust, Axum, sqlx on Postgres)

- `sign_in.rs`, `google.rs`: signing in. `/mcp/callback` is where an MCP server's sign-in comes back.
- `api.rs`: what clients, servers and the web app call.
- `config.rs`: the install command a token comes with. `pages.rs`: the page a failed sign-in ends on.
- `migrations/`: the schema. `e2e/`: the real router on a fresh database per test.
- `DEV_LOGIN=1` lets anyone sign in as anyone. It is for tests, the demo and local work, and must
  never be set in production.

### apps/gateway (Rust, the `motile-gateway` image)

Runs from `ghcr.io/motileapp/motile-gateway` with a volume at `/data`, linked with
`motile-gateway setup <code>` or `MOTILE_ENROLL_CODE`.

- `serve.rs`, `access.rs`: the iroh endpoint, for the account's servers and clients only.
- `router.rs`: which account a thread's requests go to.
- `upstream.rs`, `websocket.rs`: the HTTP requests and Codex's WebSockets.
- `oauth.rs`, `seal.rs`: the accounts' sign-ins and their sealed tokens.
- `limits.rs`, `usage.rs`, `pricing.rs`: the accounts' limits, usage and cost.
- `mcp/`: the account's MCP servers: passing calls through, permissions, sign-in.
- `store.rs`: its SQLite. `tests/e2e.rs`: the gateway against fake upstreams.

### packages/theme

`tokens.json` is the one source of every colour, opacity, shadow, radius and text size in the
repo. `pnpm tokens` writes it out for every program, and the map styles beside it. A colour only
one place needs is still added to the JSON, never written where it is used.

### apps/marketing (Astro, static) and apps/web (TanStack Start)

One pnpm workspace, both on shadcn/ui (preset `b1VlIvUO`): add a component with
`pnpm dlx shadcn@latest add <name>` in the app, then rewrite it on our tokens. Both take their
colours from `packages/theme/theme.css`; an opacity is one of the tokens
(`bg-overlay/(--opacity-overlay)`).

- `apps/marketing`: ships no JavaScript but the theme switch, the download button and the demo.
  `src/components/demo` is the Mac app's window in React. `public/i` is the server's installer,
  `public/linux` the GPUI app's. `pnpm --filter motile-marketing icons` draws the icons and
  `pnpm --filter motile-marketing preview-image` the link preview; run that when the demo changes.
- `apps/web`: `src/server/auth.ts` holds the session, `src/lib/account.ts` the server functions,
  `src/routes/auth/` the sign-in.

### apps/server

- `hub.rs`, `hub/`: the live state of every thread. `hub/children.rs` is the agents an agent
  starts, `hub/threads.rs` the motile tools that find and change threads.
- `mcp.rs`: the `motile` MCP server the agents call.
- `agents/`: running Claude Code and Codex and parsing what they say.
- `agent_accounts.rs`, `sign_in.rs`: the agents' accounts and signing them in.
- `handoff.rs`: what an agent is told of a thread that went on without it.
- `actors.rs`: who took each action in a thread, and labelling the ones from before.
- `store.rs`: SQLite. `sync.rs`: the change log and the copies of other servers' logs.
- `activity.rs`, `search.rs`, `pricing.rs`, `limits.rs`: the activity, message search, prices and
  the accounts' limits.
- `git.rs`, `pull_requests.rs`, `github.rs`: git, pull requests and GitHub.
- `linear.rs`, `ricrac.rs`: Linear and Ricrac. `MOTILE_LINEAR_URL`, `MOTILE_RICRAC_URL` stand in
  for them.
- `ui.rs`, `ui_tools.rs`, `ui_lookups.rs`: the programs agents show, the tools they call, their
  photos and routes.
- `files.rs`, `media.rs`, `icons.rs`: files, media and project icons.
- `title.rs`, `drafts.rs`, `generate.rs`: titles, commit messages, pull request texts, branch names.
- `access.rs`, `backup.rs`, `gateway.rs`: the account, the S3 backup, the gateway.
- `pacing.rs`, `setup.rs`, `service.rs`, `update.rs`.
- `tests/e2e.rs`: the server against `scripts/fake-agent` over real iroh connections.

### crates/core

A static library for the clients (`ffi.rs`: JSON commands in, JSON events out) and a Rust library
for tests. `api.rs` is the JSON the client and the core exchange.

- `core.rs`: one loop that owns all state.
- `link.rs`, `away.rs`, `sync.rs`, `backup.rs`: connections, coming back, carrying threads between
  servers, the backup.
- `cache.rs`, `outbox.rs`, `follow.rs`, `media.rs`, `log.rs`: the local copy, messages on their
  way, media and the client's log.
- `engaged.rs`, `usage.rs`, `activity.rs`, `limits.rs`, `gateway.rs`, `mcp_servers.rs`: the usage
  view and the gateway pages.
- `pull_request.rs`, `linear.rs`, `ricrac.rs`, `git.rs`, `browse.rs`, `skills.rs`, `snooze.rs`,
  `search.rs`, `markup.rs`: what those parts of the clients show.
- `render/`: transcripts as rows ready to draw. `render/openui/` runs the programs and resolves
  them into the trees the clients draw; `canvas/` lays out charts and drawings, `map.rs` the map's
  camera. The golden trees are `fixtures/openui` (`OPENUI_BLESS=1` rewrites them). After changing
  the types or the font, run `scripts/ui-types.sh` and `scripts/lucide-names.mjs`.
- `fixtures/frames`: the layout references CI holds the clients to. `scripts/compare-frames.sh`
  compares them; `--bless` takes the Mac's and iOS's frames after a layout change.
- `examples/drive.rs` drives the core from a terminal; `examples/seed.rs` makes the dev data.

### packages/apple (Swift: SwiftUI, with AppKit and UIKit for the transcript)

`MotileKit`, the Swift package both clients are made of. `Shared` is what both use, `Mac` and
`iOS` what only one does. A view that has to be AppKit on the Mac and UIKit on iOS has a twin in
each, named alike (`KitMac.swift`, `KitIOS.swift`).

- `Shared/Platform`: `Platform.swift` hides what the systems do differently. Sizes are written as
  on the Mac; `Platform.scale` enlarges them on iOS. `Symbol.swift` is the icons, from Lucide's
  font; a new one is a case with its character from lucide-static's `font/codepoints.json`.
- `Shared/Core`: `CoreBridge.swift` calls the core, `AppStore.swift` is the state the views show.
- `Shared/Theme/Theme.swift`: the tokens. Every colour, text size, opacity, shadow and radius is
  one of them. A view lies on a `Surface`, and what it draws takes that surface's accent.
- `Shared/Views/UI`: the components every view is made of. Every line to type in on the Mac is a
  `LineField`, with `onReturn` and `onLineKey`.
- `Shared/Views/Transcript`: the transcript. `TranscriptView.swift` only keeps views for the rows
  on screen, `Rows.swift` prepares them off the main thread, `RowViews.swift` draws them.
- `Shared/Views/…`: `Sidebar`, `Composer`, `Thread`, `Panel`, `Settings`, `Usage`, `Markup`,
  `Onboarding`, `CommandPanel.swift`.
- `Mac/`, `iOS/`: each client's layout, and `DemoDriver.swift`, `DemoDriverIOS.swift`.

### apps/android (Kotlin, Jetpack Compose)

The iOS app in Kotlin, from Android 10. Run `./gradlew` from `apps/android` with a JDK 21.
Package `app.motile`, application `app.motile.motile`.

- `core/`: the core's JNI crate. `scripts/build-core.sh` builds it; Gradle runs it before every
  build, and `-Pmotile.skipCore` keeps the last one.
- `app/src/main/java/app/motile/`: `core/` is `Shared/Core`, `theme/` is `Theme.swift`,
  `platform/` is `Platform.swift` and `Symbol.swift`, `ui/` is the component kit, and the other
  folders are the views of the same names. `layout/` lays out phone and tablet.
- `app/src/test` tests the models, `gestures/` uses the client with fingers.
- `scripts/release.sh` builds the signed APK.

### apps/macos and apps/ios

- `apps/macos`: the Mac app's executable, `scripts/build-app.sh` and `scripts/ci-demo.sh`.
- `apps/ios`: the Xcode project, for iOS 18 and later. `scripts/build-core.sh`, `MotileUITests`,
  `scripts/testflight.sh`.

### apps/gpui (Rust: GPUI with GPUI Kit)

The Mac app in Rust, the client on Linux and Windows. A Cargo workspace of its own: run `cargo`
from `apps/gpui`. `scripts/package.sh` builds what a release carries.

GPUI and wgpu carry patches of ours in `patches/`. `scripts/patch-crates.sh` unpacks them into
`patched/`; run it before `cargo`. To change a patch, edit its crate in `patched/`, then run
`scripts/patch-crates.sh --save`.

- `bridge.rs`: runs the core. `store/` is the state the views show, as `AppStore.swift` is.
- `root.rs`, `main_view.rs`, `app_menu.rs`, `keyboard.rs`: the window, its layout, the menus and
  the keys.
- `theme.rs`: `Theme.swift` in Rust. `ui/`: the component library.
- `transcript/`, `composer/`, `sidebar/`, `thread/`, `panel/`, `settings/`, `media/`: the views.
- `scripts/dev-app.sh`, `scripts/shot.sh`, `scripts/do.sh`: its dev app on the Mac's dev stack;
  `--mac` opens the Mac's beside it.

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
- A machine that runs agents is a server, in the code and in what the user reads. The auth server
  is always called the auth server.
- Rendering logic belongs in `crates/core`, not in a client, so that every client gets it.
- The marketing site's demo (`apps/marketing/src/components/demo`) shows the Mac app as it is.
  When a change alters anything it shows, update the demo in the same change.
- The Mac app, the iOS app, the GPUI app and the Android app do the same things. What one gets, the
  others get in the same change. What the two Apple clients do is written once, in
  `packages/apple/Sources/MotileKit/Shared`; the GPUI app writes it again in Rust as the Mac app
  does it, and the Android app in Kotlin as the iOS app does it.
- A button, a menu, a spinner, a chip or a field in the clients comes from
  `packages/apple/Sources/MotileKit/Shared/Views/UI`, `apps/gpui/src/ui` in the GPUI app, or
  `apps/android/app/src/main/java/app/motile/ui` in the Android app. Its
  size is a `ControlSize`, never a number, and nothing is smaller than `.small`. A view does not
  style a control by hand; what is missing is added to the component. A button inside something is
  as far from its top and bottom as from its side. A button that waits is `pending`: it shows the
  spinner, or turns its arrow if its symbol `turns`, and a spinner is never followed by "…".
- A gap between things of a kind side by side (a grid's cells, a chart's bars, segments, rows) is
  only drawn. What each one hovers, clicks and taps on grows to meet its neighbours', so the pointer
  never falls through between them.
- Do not leave paragraphs of comments on top of the code. You should try to avoid them as much
  as possible with understandable function names and code. If they are necessary even then, make
  them concise. Remove such comments when you come by them in the codebase. Comments should
  always move with code, not be left behind.
- Use guard statement patterns in any code you write.
- Do not edit generated code: `Cargo.lock`, `pnpm-lock.yaml`, `apps/web/src/routeTree.gen.ts`,
  what `pnpm tokens` writes (`tokens.css`, `theme.css`, `Tokens.swift`, `apps/gpui/src/tokens.rs`,
  `Tokens.kt`, `res/values*/tokens.xml`, `map-style-*.json` and their copies), what
  `scripts/ui-types.sh` writes (`UiNodes.swift`, `UiNodes.kt`) and what `scripts/lucide-names.mjs`
  writes (`crates/core/src/render/openui/tree/lucide.rs`). The shadcn components in
  `src/components/ui` are ours once added. Never edit an applied migration; add a new one.
- Do not write useless tests; tests should cover input/output behaviour.
- What we take from another project keeps its licence beside it.
- Reinvent the wheel but do not reinvent the car. If you are solving a simple problem do not
  introduce a library. If you are solving a complex but common problem, there is likely a
  modern library for it, if so, use it.
- Do not start editing code in response to a question. We'll tell you when to edit code.
- Do not open a dev app or a simulator unless we ask for it or it's necessary for your work.
- The app's screenshots and videos you show us should be in dark mode unless we say otherwise.
- If we are missing a glaring issue when we ask you to do something, do not hesitate to
  point it out.
- Never commit or push code unless explicitly asked to do so.
- Never make a PR unless explicitly asked to do so.
- Do not insert yourself into our code, commits or PRs in any way. Our codebase is not your
  ad space.
- When a deploy changes required env vars, deploy the code first and change the variables
  after. Unbind restarts the running pod on every variable change.
- Do not slow the machine to a crawl; other sessions work on it too. Before a dev app, a simulator
  or a big build, check what runs and how much memory is free (`memory_pressure`,
  `xcrun simctl list devices booted`, `pgrep -fl build/dev`), and wait if there is no headroom.
  Use one simulator or emulator at a time, and stop what you started (`scripts/dev-app.sh --stop`).
- After you make code changes, run the checks below and fix what they raise.
- A change must not break CI: the checks, the e2e tests, the iOS UI tests and the Mac app's demo
  (`DemoDriver.swift`). When a change touches what one of them does, update it in the same change
  and run it: `apps/macos/scripts/build-app.sh`, then `apps/macos/scripts/ci-demo.sh`, then read
  `screenshots/checks.txt`. A push to `main` with a red workflow blocks the release.

## Development

Needs Rust stable, Docker (for Postgres), and Node 24 with pnpm. The Mac and iOS apps need Xcode 26
or later, the iOS app also `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`. The
Android app needs a JDK 21, the Android SDK with platform 37, the NDK, an emulator image,
`cargo install cargo-ndk` and `rustup target add aarch64-linux-android x86_64-linux-android`.

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
    (cd apps/gpui && scripts/patch-crates.sh && cargo fmt && cargo clippy --all-targets && cargo test)
                                                          # after changing the GPUI app, crates/core or crates/protocol
    (cd apps/android && ./gradlew lintDebug testDebugUnitTest assembleDebug)
                                                          # after changing the Android app, crates/core or crates/protocol
    scripts/compare-frames.sh                             # after changing how a client lays out a program

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
    scripts/do.sh "slide Tip 1" "press Alex"  # drags a program's slider, presses its button
    scripts/ui-test.sh                      # the tests that swipe and tap (MotileUITests)
    MOTILE_SIM="iPad Pro 13-inch (M5)" scripts/dev-app.sh    # another simulator

    # from apps/android, in the emulator, which has no window; works with the Mac's screen locked
    scripts/dev-app.sh                      # builds what changed and opens the client
    scripts/shot.sh shot.png                # a picture of its screen
    scripts/do.sh "sidebar open"            # has the client do something; the steps are in DemoDriver.kt
    scripts/do.sh "slide Tip 1" "press Alex"  # drags a program's slider, presses its button
    scripts/ui-test.sh                      # the tests that swipe and tap (GestureTests.kt)
    scripts/check.sh [path]                 # compiles the Kotlin and lists its errors
    MOTILE_AVD=tablet scripts/dev-app.sh    # another emulator, made the first time

Both are signed in as `demo@motile.app` on a local auth server with the dev login. Their server,
`studio`, runs `scripts/fake-agent` and starts with a project and three finished threads. It all
lives in `apps/macos/build/dev`, with its ports in `build/dev/ports`. Other trees run their own
dev apps at the same time: never pick ports by hand, delete another tree's port claims, or stop a
process that isn't in your tree's `build/dev`. On the Mac, type with `osascript` and address the
client by the pid in `build/dev/app.pid`. Judge speed with `scripts/ci-demo.sh`, not the dev app.

### The fake agent

`scripts/fake-agent` stands in for Claude Code and Codex in the tests, the demo and the dev
clients. It replays the recorded output in `fixtures/` or makes up a turn, depending on the prompt:

| Prompt | What it does |
| --- | --- |
| "watch the deploy" | Stays after its turn, as Claude Code does while it monitors, and starts agents meanwhile when asked "ask agents in the background" |
| "run greet.py" | Under supervised access, asks before each tool call |
| "show the screenshot" | Makes an image, reads it (the Read row shows it when opened) and shows it |
| "show the recording" | Shows a video with its preview image |
| "send the report" | Sends a file to download |
| "greet by name" | Really changes files in its folder |
| "ask two agents" (Claude Code), "ask an agent" (Codex) | Starts agents that say what they do and report |
| "ask another account" | Starts Codex through `start_agent` to look over greet.py and waits for it (Claude Code) |
| "ask agents in the background" | Starts Codex and Claude Code through `start_agent` and ends; told one finished, says what it found (Claude Code) |
| "ask three agents" | Starts three agents through `start_agent`, the last of which fails (Claude Code) |
| "ask sixty agents" | Starts fifty-four agents that say hello and six that look over greet.py slowly, through `start_agent` (Claude Code) |
| "ask a supervised agent" | Starts a supervised agent that runs greet.py, which waits for the user's approval (Claude Code) |
| "ask a pinned agent" | Starts two agents that look over greet.py slowly, one pinned to one of the gateway's Claude Code accounts and one the gateway places, through `start_agent` (Claude Code) |
| "ask an agent which color" | Starts an agent that asks which color; told it asks, answers blue through `answer_agent` (Claude Code) |
| "ask an agent to change the settings" | Starts an agent that asks before it edits `.claude/settings.json` under any access; told it waits, allows it through `approve_agent` (Claude Code) |
| "ask a background agent" | Starts an agent in the background and ends its turn while it works; once the agent reports, wakes by itself and says what it found (Claude Code) |
| "look over" | Reads greet.py and runs the tests, a step every `FAKE_AGENT_CHILD_STEP` seconds, and says what it would change |
| "run the workflow" | Runs a workflow of two phases, whose last agent fails (Claude Code) |
| "wait for more" | Sent now while it writes, waits for the next message sent now, which stops it before its answer, so it is never repeated (Claude Code) |
| "too late" | Sent while it works, is refused once the turn ends, as a steer that comes as the turn ends is (Codex) |
| "which color" | Asks the user a question |
| "ask a few questions" | Asks three questions at once, one with a recommended option and one that takes several (Claude Code) |
| "plan the hello" | Presents a plan to approve |
| "hit the limit" | Is stopped by the usage limit, which resets `FAKE_AGENT_RESET` seconds later |
| "agents hit the limit" | Is stopped by the usage limit while two agents it started work, which it stops after the turn (Claude Code) |
| "crash before taking this" | Exits before it takes the prompt (Claude Code) |
| "show the ui <name>" | Shows the program `fixtures/openui/<name>.oui`, streamed |
| "show the dashboard" | Shows a program that reads `git_log` and `usage` with `Query` |
| "ask with a form" | Shows a form that sends its values with `@ToAssistant`; told them, says what they are |
| "show a broken ui" | Shows a program with an unknown component and a missing prop; told its errors, replies with a patch that fixes them (Claude Code) |
| "$ship-it", "$review" | Says it followed the skill: the project's in `.claude/skills` (`.agents/skills` for Codex), or one it makes up |

As `claude auth login` it takes any code without "wrong" in it.

Other stand-ins in the dev apps: `scripts/fake-gh` for `gh` (its pull requests in
`build/dev/github.json`, which you can edit), `scripts/fake-s3` for S3 (endpoint
`http://127.0.0.1:<S3_PORT>`), `scripts/fake-linear` and `scripts/fake-ricrac` for Linear and
Ricrac (`MOTILE_DEV_LINEAR=real` or `MOTILE_DEV_RICRAC=real` after a `--stop` uses the real ones),
and `scripts/fake-upstream` for Anthropic, OpenAI and two MCP servers behind the dev gateway. Each
refuses a code or key with "wrong" in it.

## Releasing and deploying

Every push to `main` that touches the server, the auth server, the gateway, the Mac app, the GPUI
app or the Android app is a release: the `Release` workflow publishes everything as a GitHub
release. Its version is the first two numbers of `version` in `Cargo.toml` and the commit count as
the third (`scripts/release-version.sh`), so set `version` only to change the first two. The Mac
app is signed and notarized with the `APPLE_*` secrets; the gateway's image,
`ghcr.io/motileapp/motile-gateway`, is built once the release exists.

The Android app is signed with the key in the `ANDROID_KEYSTORE`, `ANDROID_KEYSTORE_PASSWORD` and
`ANDROID_KEY_ALIAS` secrets. Every release must be signed with that one key, or installed clients
can't update. Without the secrets, a release carries no Android app.

Every push to `main` that touches the iOS app uploads it to TestFlight
(`apps/ios/scripts/testflight.sh`). Its signing certificate and profile, in the `IOS_*` secrets,
expire on 2027-10-04.

Unbind builds the `Marketing`, `Web` and `Auth` services from `main` and deploys them on every
push that touches their files.

## Commit Messages

Commit messages start with the part of the system they touched, followed by a short imperative
sentence describing the change:

    auth: Refuse a sign-in code that was started by another client
    macos: Show the server's round-trip time in the sidebar
    server | core | macos: Stream replies in finished blocks

The parts are the folders in `apps`, `crates` and `packages`: `auth`, `marketing`, `web`, `server`,
`macos`, `ios`, `android`, `apple`, `gpui`, `gateway`, `core`, `openui` and `protocol`. Use `ci` for the workflows and `docs` for README.md and AGENTS.md.

The title should be concise. Description should explain the work in more detail (only if
required) while still being concise. Use simple language, do not try to sound smart.
