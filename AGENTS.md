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
| Marketing site (`apps/marketing`) | motile.app, as static files | The landing page, privacy and terms, and the installer at `/i` |
| Web app (`apps/web`) | app.motile.app | Lists an account's servers and clients, adds servers, removes devices |
| Server (`apps/server`, the `motile` binary) | The user's Linux machines and Macs | Runs the agents, stores threads in SQLite, serves the account's clients |
| Mac app (`apps/macos`) | The user's Mac | The interface |
| iOS app (`apps/ios`) | The user's iPhone and iPad | The interface |
| Apple kit (`packages/apple`) | Inside the Mac and the iOS app | Their state and their views |
| GPUI app (`apps/gpui`) | The user's Linux machines and Windows PCs | The Mac app again, in Rust with GPUI |
| Android app (`apps/android`) | The user's Android phones and tablets | The iOS app again, in Kotlin with Jetpack Compose |
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
  client or server could no longer understand the other. It has the change logs the servers copy
  each other's threads with too (`Watermarks`, `Changes`, `Apply`), which the clients carry.
- `auth_api.rs`, `auth_client.rs`: the auth server's JSON and the client for it.
- `identity.rs`: the device key and request signing.
- `media.rs`: names an image, a video or a file an agent sends by its contents.
- `skills.rs`: how a message names the skills it starts, `$name` at the start of a word.

### apps/auth (Rust, Axum, sqlx on Postgres)

- `sign_in.rs`: the browser's side of a sign-in. `google.rs` is the OIDC exchange.
- `api.rs`: what clients, servers and the web app call. A caller is a device that signed the
  request or a session's `Bearer` token. A server removed from an account is kept in
  `removed_servers` with its name, so that its threads, which live on as copies on the account's
  other servers, still show it; deleting it completely marks it there, and every server and client
  then drops its copies. Linking the key again makes it an ordinary server. The clients call these
  "Unlink Server" and "Delete".
- `config.rs`: builds the install command a token comes with.
- `pages.rs`: the page a failed sign-in ends on, drawn like the web app's with the colours of
  `packages/theme/tokens.css` and DM Sans from `assets/`.
- `migrations/`: the schema.
- `e2e/`: the real router on a fresh database per test, with a fake Google. `e2e/whole.rs`
  runs all three programs together.
- `DEV_LOGIN=1` lets anyone sign in as anyone without Google. It is for tests, the demo and
  local work, and must never be set in production.

### packages/theme

The design tokens. `tokens.json` is the one source of every colour, opacity, shadow and radius
in the repo; `pnpm tokens` (`build.mjs`) writes it out as `tokens.css` (plain variables, for the
auth page), `theme.css` (Tailwind, for the web projects), the clients' `Tokens.swift` and the
GPUI app's `apps/gpui/src/tokens.rs`, and the Android app's `Tokens.kt` with the few colours Android
itself draws before the app does (`res/values/tokens.xml` and `res/values-night/tokens.xml`). A colour that only one place needs, like the window buttons
of the demo or the icons' tile, is added to the JSON, never written where it is used.

### apps/marketing (Astro, static) and apps/web (TanStack Start)

One pnpm workspace. Both use shadcn/ui (preset `b1VlIvUO`); add components with
`pnpm dlx shadcn@latest add <name>` inside the app, then rewrite the component on our tokens:
shadcn's own colours, opacities and radii are gone from the theme.

- Both take their colours from `packages/theme/theme.css`: the design tokens as Tailwind
  colours, shadows and radii. An opacity is one of the tokens, as Tailwind's own modifier:
  `bg-overlay/(--opacity-overlay)`, `opacity-(--opacity-disabled)`. Dark is the default; the
  theme switch keeps the choice in a cookie on motile.app, which both read.
- `apps/marketing` ships no JavaScript but the theme switch, the download button and the demo.
  The download button (`src/components/download-button.tsx`) offers the client for the reader's
  system and every other one behind its chevron, from `DOWNLOADS` in `src/lib/links.ts`.
  `src/components/demo` is the Mac app's window in React, with made-up threads. `public/i` is the server's installer, and `public/install.sh` its old address.
  `public/linux` installs the GPUI app on Linux, which the download button copies as a command.
  `scripts/icons.mjs` draws the icons of both web projects: `pnpm --filter motile-marketing icons`.
  `scripts/preview.mjs` draws the link preview image of both from the `/preview/` page, the
  tagline over the demo: `pnpm --filter motile-marketing preview-image`. Run it when the demo
  changes.
- `apps/web`: `src/server/auth.ts` holds the session (the browser only gets an HttpOnly
  cookie), `src/lib/account.ts` is the server functions the pages call, and `src/routes/auth/`
  starts and finishes a sign-in.

### apps/server

- `agent_accounts.rs`: the agents' accounts. Each is the folder its CLI keeps the sign-in in
  (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`) and the variables its agent is given; every agent has a
  default one. A Codex account can share the default one's folder but for its sign-in. A thread
  works with one account and can move to another.
- `sign_in.rs`: signs a Claude Code account in for a client: it runs `claude auth login` with the
  account's folder, the client opens the page it links to, and the code that page shows comes
  back to it.
- `hub.rs`: the live state of every thread: turns, queued messages, approvals, monitoring,
  worktrees, snapshots and the agents an agent starts. It continues a thread once its usage limit
  resets, and after a restart that cut its agent off, where the server's settings say so. Claude
  Code's process rests for ten minutes after a turn to take the thread's next message, and a new
  one holds its first prompt until its MCP servers, claude.ai's connectors too, have connected. Every
  server has a "No project" project, whose threads each work in a folder of their own under
  `no-project` in its data folder, out of reach of any git repository above it.
- `handoff.rs`: what an agent is told of a thread that went on without it. A thread keeps a
  session for each agent and sessions folder it ran in (`thread_sessions`); going back to one
  resumes it with what it missed, a new one is told everything before. Items are told whole, as
  many as the model's context has room for, Codex's as messages (`thread/inject_items`). The
  transcript shows the switch as a `Handoff` item.
- `mcp.rs`: the `motile` MCP server on the loopback interface, which an agent calls by the token of
  its own thread: `read_thread` reads any thread of the server, for what a handoff left out, and
  `list_accounts`, `start_agent`, `agent_status`, `message_agent`, `answer_agent` and `stop_agent`
  start and follow agents on any of the server's accounts, Claude Code's or Codex's.
- `hub/children.rs`: those agents. Each works in a thread of its own, which knows the thread and
  the tool call that started it (`parent`); the sidebar doesn't list it. It gets only its prompt,
  works in its parent's folder unless given a worktree, and never has more access than its parent.
  Its parent's tool call follows how it does, and the parent's agent is told (`Notice`) when the
  turn it gave the thread ends or the thread asks a question, unless a call of its own waited for
  that. Stopping or deleting a thread stops or deletes the ones it started.
- `pacing.rs`: passes a streamed reply on in finished blocks.
- `agents/`: builds the command for a turn and parses its output into `AgentEvent`s
  (`claude.rs`, `codex.rs`). `skills.rs` asks each account's CLI for the skills it offers in a
  project (Claude Code's `initialize`, Codex's `skills/list`), which `hub.rs` keeps for half a
  minute; a message's `$name`s of them are kept with it, Codex reads them itself and Claude Code is
  given the last as the `/name` command it runs, with the rest of the message as its arguments. `models.rs` asks each account's CLI what its picker lists and the
  effort each model runs at by default (`ask.rs` asks a CLI without running a turn); `hub.rs`
  keeps the answer and asks again when a client connects or opens a draft, at most once a minute.
- `store.rs`: SQLite. Every item has a position (`seq`) and the revision that last changed it
  (`rev`); a client asks for what changed after the revision it has. It also keeps what the
  agents spend, by model and by what it was spent on (a turn, or writing a title, a branch's
  name, a commit message or a pull request): Claude Code reports a session's total with each
  result and Codex what each answer took. The sidebar's search finds threads by their messages
  here, each by its best one, cut to the part around the match.
- `pricing.rs`: what the models cost at the API's prices, fetched from LiteLLM's list once a day,
  to say what the tokens Codex spent would have cost. Claude Code says what its own cost.
- `limits.rs`: how much of their plans the agents' accounts have used, as their CLIs say without
  spending tokens: `codex app-server` answers `account/rateLimits/read` and Claude Code the
  `get_usage` control request. `hub.rs` keeps the answer for five minutes.
- `git.rs`: branches, worktrees, status, commit, pull, push, pull requests (through `gh`),
  snapshots and patches.
- `pull_requests.rs`: what GitHub says of a pull request (one `gh api graphql` read), the
  repository's list of them, and what the pull request tab does to one: merge, auto-merge, update
  its branch, close, revert, review its lines, edit, react. `hub.rs` watches a thread's pull
  request for its agent and settles the thread once it merges. Where the server's settings say
  so, it fast-forwards a project's folder on its default branch after each fetch and each merge.
- `files.rs`: browses the server's folders, takes uploads, and reads the files of a thread's
  folder, never outside it.
- `media.rs`: keeps the images and videos threads show and the files agents send, named by their
  contents. An agent sends any file as a Markdown image of its path, and gives a video its
  preview image as the image's title.
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
- `ricrac.rs`: the same for Ricrac (ricrac.app, our own tracker), through its REST operations
  (`POST /v1/ops/<name>`) where Linear's is GraphQL. `MOTILE_RICRAC_URL` stands in for Ricrac.
- `icons.rs`: finds a project's icon in its folder.
- `access.rs`: asks the auth server which clients belong to the account, and which servers it
  deleted completely.
- `sync.rs`: the server's change log and its copies of the account's other servers' logs. Triggers
  give each write to a project, a thread, an item or the usage the next number of the log
  (`migrations/0018_sync.sql`), and a delete leaves a tombstone. A client asks a server what changed
  after a number and hands the batch, compressed, to another server, which keeps only what its copy
  lacks, so a batch that comes twice changes nothing. A copy keeps its origin's numbers and can be
  passed on to a third server. The copies are apart from the server's own threads: agents,
  `read_thread` and git never see them. A new thread can go on from a copied one, which it starts
  with the copy's transcript.
- `setup.rs`, `service.rs`: what the installer runs, and the systemd or launchd service.
- `update.rs`: replaces the server's own program with the latest release.
- `tests/e2e.rs`: runs the server against `scripts/fake-agent` over real iroh connections.

### crates/core

A static library for the clients (`ffi.rs`: JSON commands in, JSON events out) and a Rust library
for tests.

- `core.rs`: one loop that owns all state. Anything that waits on the network runs in its own
  task. A server removed from the account stays, `removed`: its threads come from the copies its
  account's other servers keep, can be read and searched, and end with a row that says it was
  removed, in place of the composer. A thread says how the threads its agent started are doing
  (`children`): how many work, and which wait for the user, for its sidebar row and the strip over
  its composer.
- `sync.rs`: carries the threads between the servers: a round a moment after a server connects or a
  thread changes asks each server how far its own log and its copies go, and moves what each lacks,
  from the origin itself or, for a removed server, from the server whose copy goes furthest. On a
  metered network only what follows a copy is carried.
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
  with its description and comments set, and the prompt that hands it to an agent. `ricrac.rs`
  is the same for the Ricrac tabs.
- `browse.rs`: browsing a server's folders by typing a path.
- `skills.rs`: the skills the composer offers while the word before its cursor starts with `$`
  or `/`, best match first, and the chips a message's `$name`s become.
- `search.rs`: the sidebar's search through every server's messages, asked a moment after the
  search stops changing. What was found stays while it still fits the search, so the list never
  empties and fills again, and its excerpts come in parts that set off the match.
- `usage.rs`: what the agents spent on every server, added up for the usage view: a series for
  each agent, and the models, projects and kinds of token.
- `limits.rs`: the Limits tab of the usage view: a section for each login, shown once however
  many servers share it, and a row for each window with when it starts over and its pace.
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
  `font/codepoints.json`. Linear's and Ricrac's logos, which Lucide doesn't have, are drawn by
  hand there.
- `Shared/Core`: `CoreBridge.swift` calls the Rust core, `AppStore.swift` is the state the
  views show, `RightPanel.swift` the right panel's state, `Linear.swift` the Linear tab's and `Ricrac.swift`
  the Ricrac tab's.
- `Shared/Views/Transcript`: the transcript. `TranscriptView.swift` only keeps views for the
  rows on screen, `Rows.swift` builds their text and measures them off the main thread,
  `RowViews.swift` are the rows.
- `Shared/Views/Sidebar`, `Composer`, `Onboarding`, `Thread`: SwiftUI. In the composer a skill is
  picked from `SkillMenuStrip.swift`, a strip over it, and becomes a chip (`SkillChip.swift`): an
  attachment in the composer's text, which is written `$name`, and in a message a box its layout
  manager draws; `Shared/Core/Skills.swift` is their state. While the sidebar
  searches it is one flat list (`SearchResults.swift`): the threads found by their title, project
  or pull request, then the ones found by a message, each row as tall whatever it shows.
- `Shared/Views/UI`: the components every view is made of. `ActionButton` and `ActionMenu` (a
  variant, one of three sizes, an optional symbol, a pending state), `Spinner`, `Chip`,
  `InputField`. Their sizes are `ControlSize` and their corners `Radius`, in `Theme.swift`.
- `Shared/Theme/Theme.swift`: the design tokens, generated into `Tokens.swift` beside it from
  `packages/theme/tokens.json`. Every colour is a `Theme` token, every opacity one of `Opacity`,
  every shadow a `ShadowSize` at one of the shadow opacities and every radius one of `Radius`. A view lies on a `Surface` (the page, a box, a card or a popover) and what it
  draws on it, a control, a row or a box, takes that surface's accent. A colour's wash is
  `.wash()`, a whole disabled view `.opacity(.disabled)`.
- `Shared/Views/CommandPanel.swift`: the panel behind ⌘K, ⌘N and ⌘P, a sheet on iOS.
- `Shared/Views/Settings`: the settings, a route over the whole client with a way back: the
  sections and a search of their groups in a sidebar (`SettingsSidebar.swift`) and the open
  section's page (`SettingsPage.swift`). The Mac lays them out in `Mac/SettingsRoute.swift`, iOS
  in `iOS/SettingsSheet.swift`, a sheet. The Integrations page turns each issue tracker's tab on
  or off on this device (`Shared/Core/Integration.swift`; Linear is on to start with, Ricrac off).
- `Shared/Views/Usage`: the usage, a route of its own opened from the sidebar's footer, with the
  servers it counts picked in its title. `UsageView.swift` holds its tabs and what they read,
  `LimitsView.swift` is the Limits tab, what the agents' plans have used, and `SpendingView.swift`
  the Cost and Tokens tabs: a chart by agent and the models, projects and kinds of token under it.
  The Mac lays it out in `Mac/UsageRoute.swift`, iOS in `iOS/UsageSheet.swift`.
- `Shared/Views/Panel`: the panel on the right of the thread: changes, files, agents
  (`AgentsView.swift`: the agents at work, the ones that ended folded under them, and the thread
  that started this one; an agent opens in the thread's place, in its own thread when it has one
  and read only with `AgentBar` otherwise) and pull
  requests (`PullRequestView.swift`, `PullRequestActivity.swift`, `PullRequestList.swift`,
  `LineCommentSheet.swift`, `PullRequestParts.swift`), Linear (`LinearView.swift`) and Ricrac
  (`RicracView.swift`).
  `CodeView.swift` draws a diff or a file.
- `Mac/`: `MotileApp.swift` lays out the window, `MediaViewer.swift`, `AppUpdater.swift`,
  `Glass.swift`, `Demo/DemoDriver.swift`.
- `iOS/`: `MotileAppIOS.swift` is the client and its layout, `Drawer.swift` the sidebar under the
  thread, `PanelScreen.swift`, `ThreadScreen.swift`, `SidebarScreen.swift`, `Tables.swift`,
  `DemoDriverIOS.swift` the steps `do.sh` runs.

### apps/android (Kotlin, Jetpack Compose)

The iOS app ported to Android, for phones and tablets, from Android 10. A Gradle project of its own:
run `./gradlew` from `apps/android`, with a JDK 21 (`JAVA_HOME`). It does what the iOS app does, the
way iOS does it, with Android's own facilities where iOS uses the system's: Custom Tabs for signing
in, the photo picker and the camera for attachments, the share sheet, Media3 for videos, the back
gesture for what Esc does. Its package is `app.motile`, its application `app.motile.android`.

- `core/`: the Rust core's JNI crate (`motile-android`, a member of the workspace), which wraps
  `crates/core/src/ffi.rs`. Its events wait in a queue the app's event thread takes them from, so no
  thread of the core calls into Java. It gives the JVM to `rustls-platform-verifier`, whose Kotlin
  half comes from its Maven repository at the version in `Cargo.lock`, and to `ndk-context`, which
  iroh's resolver reads the system's nameservers through, and sends the core's log to logcat.
  `scripts/build-core.sh` builds it with cargo-ndk into `app/build/jniLibs`; Gradle runs it before
  every build, and `-Pmotile.skipCore` leaves the one built last.
- `app/src/main/java/app/motile/`: `core/` is `Shared/Core` in Kotlin (`CoreBridge.kt`, `Models.kt`,
  `AppStore.kt`, whose state is Compose state, `Preferences.kt` for what UserDefaults keeps,
  `SignInSession.kt` for a Custom Tab that returns a `motile://` address, `AppUpdater.kt`, which
  updates the client from the latest release as the Mac app's does: it downloads the release's APK,
  checks that it is signed with the client's own key, and hands it to Android's installer), `theme/` is `Theme.swift`,
  `platform/` is `Platform.swift` and `Symbol.swift`, `ui/` is the component kit of `Shared/Views/UI`
  and `Shared/Views/Shared`, and `transcript/`, `composer/`, `sidebar/`, `thread/`, `panel/`,
  `commandpanel/`, `settings/`, `usage/`, `onboarding/` and `media/` are the views of the same names.
  `layout/` lays out the screen: on a phone the sidebar and the panel are under the thread, which a
  swipe slides aside (`Drawer.kt`), on a tablet all three are side by side (`MainScreen.kt`).
  `Keyboard.kt` runs the core's keybindings from a keyboard with keys, where `mod` is Ctrl.
  `Root.kt` shows the sheets, the viewer and the alerts. `DemoDriver.kt` runs the steps `do.sh` writes.
- `app/src/test` tests the models. `gestures/` uses the client with fingers (`GestureTests.kt`, the
  iOS app's gesture tests): a test module of its own that runs in a process of its own, so that it
  can stop the client and open it again, as the iOS app's UI tests do.
- `scripts/release.sh` builds the signed APK. Only a development build takes the dev scripts'
  variables (launch extras: `MOTILE_AUTH_URL`, `MOTILE_RELEASES_URL` and the others), since any app
  can start the client with extras. The dev app scripts are under Development.

### apps/macos and apps/ios

- `apps/macos`: the Mac app's executable and what builds it (`scripts/build-app.sh`).
  `Resources/AppIcon.icon` is the icon of both clients. `scripts/ci-demo.sh` runs the demo in CI.
- `apps/ios`: the Xcode project, for iOS 18 and later. `scripts/build-core.sh` builds the core
  for iOS, `MotileUITests` uses the client with fingers, `scripts/testflight.sh` uploads a build.
- The dev app scripts of both are under Development.

### apps/gpui (Rust: GPUI with GPUI Kit)

The Mac app ported to Rust, the client on Linux and Windows. A Cargo workspace of its own with its
own `Cargo.lock`: run `cargo` from `apps/gpui`. CI checks it on Linux and Windows, and every release
carries it for both, built by `scripts/package.sh` (`Motile-GPUI-linux-x86_64.tar.gz`,
`-linux-aarch64.tar.gz`, `-windows-x86_64.zip`, `-windows-aarch64.zip`), which its updater installs.
Every release carries it for Macs too, signed and notarized as `Motile-GPUI-macos-aarch64.zip` by the
`macOS` workflow, but motile.app doesn't offer it and its updater only opens the release's page there:
the Mac app is the client on a Mac. It uses `crates/core` as a Rust library. Its data is in
`%LOCALAPPDATA%\Motile GPUI` on Windows, `~/.local/share/motile-gpui` on Linux and `Motile GPUI`
on a Mac, so it is a device of its own. On Linux and Windows it takes `motile://` links itself, and
a second launch hands its link to the running one (`instance.rs`).

It builds on GPUI and wgpu with patches of ours, so that Linux redraws and presents only what
changed in a frame: `patches/` holds a `.patch` for each crate, and `scripts/patch-crates.sh`
unpacks the crates with them into `patched/`, which `[patch.crates-io]` points at. Run it before
`cargo`; the scripts and CI do. To change a patch, edit its crate in `patched/`, then run
`scripts/patch-crates.sh --save`. A GPUI Kit on another GPUI needs the patches
redone for its versions in `patches/crates`; the script refuses a patch that `Cargo.lock` left unused.

- `bridge.rs`: runs the core and prepares a transcript's rows off the main thread. `store/` is
  the state the views show, as `AppStore.swift` is, a module per area (`threads.rs`, `git.rs`,
  `projects.rs`, `pull_requests.rs`, `linear.rs`, `ricrac.rs`, `settings.rs`, `sidebar.rs`).
- `root.rs`, `main_view.rs`, `app_menu.rs`: the window, its layout and the menu bar.
  `keyboard.rs` runs the core's keybindings ahead of the menus and the focused view, as
  `ShortcutMonitor.swift` does, with the commands in `store/commands.rs` and the rules in
  `store/shortcuts.rs`, and does what Esc means when nothing focused claims it.
  `settings/` is the settings route over the window, with `usage.rs` the Usage chart.
- `theme.rs`: `Theme.swift` in Rust: the tokens of `tokens.rs` with the colours made from them,
  the `Surface`s and their layers, `ControlSize`, and shadows. The typeface is DM Sans, bundled in
  `assets/fonts`.
- `ui/`: the component library of `Shared/Views/UI` (`control.rs`: `ActionButton`,
  `ActionMenu`, `Spinner`, `Chip`, `Switch`, `Segmented`, `InputField`, `TextArea`, cards;
  `parts.rs`: `KeyCaps`, `CopyButton`, `CommandBox`, `PickMark`, `FadingScroll`, `CheckMenu`),
  the row light (`Highlight` in `mod.rs`), the menus the app draws itself (`menu.rs`), the modal
  stack and confirmations (`modal.rs`), the Lucide icons by the names of `Symbol.swift`
  (`icons.rs`). Nothing native is imitated: a menu or a modal is drawn in the app's own style.
- `transcript/`: `model.rs` (the rows and their splices into GPUI's list), `prose.rs`,
  `rows.rs`, `view.rs`.
- `composer/`, `sidebar/`, `thread/`, `git.rs`, `command_panel.rs`, `media/`,
  `onboarding.rs`: the views of the same names in the Apple kit. The skills' menu and chips are
  `composer/skills.rs`, `store/skills.rs` and `ui/skill_chip.rs`; GPUI Kit's inline tokens are the
  composer's chips. `app_window.rs` keeps the
  window invisible until it has something to show and its frame across launches, `lifecycle.rs`
  tells the core when the Mac wakes or changes networks. `window_frame.rs` is the window's
  buttons in the top bar where the desktop draws no title bar (GNOME under Wayland); GPUI Kit
  draws the edges that resize it.
- `panel/`: the right panel: `state.rs` (the tabs, as `RightPanel.swift`), `view.rs` (the chrome
  and the changes, files and agents tabs), `code_view.rs`, `pull_request.rs`,
  `pull_request_list.rs`, `line_comment.rs`, `linear.rs` and `ricrac.rs`.
- `scripts/dev-app.sh` opens its dev app on the Mac's dev stack (`apps/macos/scripts/dev-stack.sh`),
  a device of its own with its data in `apps/macos/build/dev/gpui`; `--mac` opens the Mac's dev
  app beside it to compare. `scripts/shot.sh` takes a picture of it, and `scripts/do.sh "click 85
  330" "key cmd-k" "type hello"` clicks, types and runs commands in it, through `dev_drive.rs`.
  `scripts/build-app.sh` builds `Motile GPUI.app`.

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
- The Mac app, the iOS app, the GPUI app and the Android app do the same things. What one gets, the
  others get in the same change. What the two Apple clients do is written once, in
  `packages/apple/Sources/MotileKit/Shared`, and only what a system does differently is written
  twice; the GPUI app writes it again in Rust, as the Mac app does it, and the Android app in Kotlin,
  as the iOS app does it.
- A button, a menu, a spinner, a chip or a field in the clients comes from
  `packages/apple/Sources/MotileKit/Shared/Views/UI`, `apps/gpui/src/ui` in the GPUI app, or
  `apps/android/app/src/main/java/app/motile/ui` in the Android app. Its
  size is a `ControlSize`, never a number, and nothing is smaller than `.small`. A view does not
  style a control by hand; what is missing is added to the component. A button inside something is
  as far from its top and bottom as from its side. A button that waits is `pending`: it shows the
  spinner, or turns its arrow if its symbol `turns`, and a spinner is never followed by "…".
- Do not leave paragraphs of comments on top of the code. You should try to avoid them as much
  as possible with understandable function names and code. If they are necessary even then, make
  them concise. Remove such comments when you come by them in the codebase. Comments should
  always move with code, not be left behind.
- Use guard statement patterns in any code you write.
- Do not edit generated code: `Cargo.lock`, `pnpm-lock.yaml`, `apps/web/src/routeTree.gen.ts`,
  and what `pnpm tokens` writes from `packages/theme/tokens.json`: `tokens.css`, `theme.css`,
  `Tokens.swift`, `apps/gpui/src/tokens.rs`, the Android app's `Tokens.kt` and its
  `res/values*/tokens.xml`.
  The shadcn components in `src/components/ui` are ours once added: they are rewritten on the
  design tokens and edited like any view. Never edit an applied migration in
  `apps/auth/migrations` or `apps/server/migrations`, add a new one.
- Do not write useless tests; tests should cover input/output behaviour.
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
- Do not slow the machine to a crawl. Other sessions work on it at the same time. Before you
  start a dev app, a simulator or a big build, look at what already runs and how much memory is
  free (`memory_pressure`, `xcrun simctl list devices booted`, `pgrep -fl build/dev`). If there
  is headroom, go ahead. If not, wait for it instead of adding to the pile. Use one simulator or
  emulator at a time, and when you are done, stop what you started: `scripts/dev-app.sh --stop`,
  which on iOS also shuts its simulator down, and on Android its emulator.
- After you make code changes, run the checks below and fix what they raise.
- A change must not break what CI runs on it: the checks, the e2e tests, the iOS UI tests and
  the Mac app's demo (`DemoDriver.swift`, run by `scripts/ci-demo.sh` in the `Release` and
  `macOS` workflows), which signs in, adds a project, and drives threads, git and drafts end to
  end. When a change touches what one of them does or expects, read it, update it in the same
  change, and run it (`apps/macos/scripts/build-app.sh`, then `apps/macos/scripts/ci-demo.sh`,
  then read `screenshots/checks.txt`)
  before calling the change done. A push to `main` with a red workflow blocks the release.

## Development

Needs Rust stable, Docker (for Postgres), and Node 24 with pnpm for the marketing site and the
web app. The Mac app and the iOS app need a Mac with Xcode 26 or later, the iOS app also
`rustup target add aarch64-apple-ios aarch64-apple-ios-sim`. The Android app needs a JDK 21, the
Android SDK with platform 37, the NDK and an emulator image (`sdkmanager`), `cargo install cargo-ndk`
and `rustup target add aarch64-linux-android x86_64-linux-android`.

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

    # from apps/android, in the emulator, which has no window; works with the Mac's screen locked
    scripts/dev-app.sh                      # builds what changed and opens the client
    scripts/shot.sh shot.png                # a picture of its screen
    scripts/do.sh "sidebar open"            # has the client do something; the steps are in DemoDriver.kt
    scripts/ui-test.sh                      # the tests that swipe and tap (GestureTests.kt)
    scripts/check.sh [path]                 # compiles the Kotlin and lists its errors
    MOTILE_AVD=tablet scripts/dev-app.sh    # another emulator, made the first time

Both are signed in as `demo@motile.app` on an auth server with the dev login, never a real
account. Their server, `studio`, runs `scripts/fake-agent` and starts with a project, which has two skills of its own, and three
finished threads. All of it lives in `apps/macos/build/dev` and keeps running until you stop it,
which you do when your task is done; its ports are in `build/dev/ports`. Other sessions run
their own dev apps from other trees at the same time: each tree claims its ports in
`~/Library/Caches/motile-dev/ports` (`apps/macos/scripts/claims.sh`), and `dev-app.sh` moves off
ports another tree claimed on its own. `ci-demo.sh` claims its ports too and waits for another
tree's demo to finish. Never pick ports by hand, delete another tree's claims, or stop or kill a process that isn't
in your tree's `build/dev`. On the Mac, type with `osascript` (System Events
`keystroke`, which needs Accessibility; pictures need Screen Recording) and address the client by
the pid in `build/dev/app.pid`. The Mac's dev app is built without whole-module optimization, so
that it builds fast; judge speed with the demo (`scripts/ci-demo.sh`), and use it only for that.

### The fake agent

`scripts/fake-agent` stands in for Claude Code and Codex in the tests, the demo and the dev
clients. It replays the recorded output in `fixtures/` or makes up a turn, depending on the prompt:

| Prompt | What it does |
| --- | --- |
| "watch the deploy" | Stays after its turn, as Claude Code does while it monitors |
| "run greet.py" | Under supervised access, asks before each tool call |
| "show the screenshot" | Makes an image and shows it |
| "show the recording" | Shows a video with its preview image |
| "send the report" | Sends a file to download |
| "greet by name" | Really changes files in its folder |
| "ask two agents" (Claude Code), "ask an agent" (Codex) | Starts agents that say what they do and report |
| "ask another account" | Starts Codex through `start_agent` to look over greet.py and waits for it (Claude Code) |
| "ask agents in the background" | Starts Codex and Claude Code through `start_agent` and ends; told one finished, says what it found (Claude Code) |
| "ask three agents" | Starts three agents through `start_agent`, the last of which fails (Claude Code) |
| "ask sixty agents" | Starts fifty-four agents that say hello and six that look over greet.py slowly, through `start_agent` (Claude Code) |
| "ask a supervised agent" | Starts a supervised agent that runs greet.py, which waits for the user's approval (Claude Code) |
| "ask an agent which color" | Starts an agent that asks which color; told it asks, answers blue through `answer_agent` (Claude Code) |
| "look over" | Reads greet.py and runs the tests, a step every `FAKE_AGENT_CHILD_STEP` seconds, and says what it would change |
| "run the workflow" | Runs a workflow of two phases, whose last agent fails (Claude Code) |
| "which color" | Asks the user a question |
| "plan the hello" | Presents a plan to approve |
| "hit the limit" | Is stopped by the usage limit, which resets `FAKE_AGENT_RESET` seconds later |
| "agents hit the limit" | Is stopped by the usage limit while two agents it started work, which it stops after the turn (Claude Code) |
| "crash before taking this" | Exits before it takes the prompt (Claude Code) |
| "$ship-it", "$review" | Says it followed the skill: the project's in `.claude/skills` (`.agents/skills` for Codex), or one it makes up |

As `claude auth login` it signs in with any code but one with "wrong" in it, as the folder's
name at motile.app. A message sent now while it works is read after its next tool call, or stops a reply that
streams (Claude Code). Asked for a title, a commit message, a pull request's text or a branch's
name, it writes one.

`scripts/fake-gh` stands in for GitHub's `gh` in the tests and the dev apps (through
`MOTILE_GH_PATH`), whose project has a local `origin` to push to. It keeps its pull requests in
`build/dev/github.json`; edit that file to see a pull request fail its checks, conflict, wait for
a review, have conversations on its lines or sit in a stack, and set `delay` to see a client wait.

`scripts/fake-linear` stands in for Linear in the dev apps (through `MOTILE_LINEAR_URL`), whose
server starts out connected to its workspace. It keeps its team, users and issues in
`build/dev/linear.json`. `MOTILE_DEV_LINEAR=real scripts/dev-app.sh`, after a `--stop`, leaves
Linear the real one, to connect a workspace of your own. `scripts/fake-ricrac` does the same for
Ricrac (`MOTILE_RICRAC_URL`, `build/dev/ricrac.json`, `MOTILE_DEV_RICRAC=real`).

## Releasing and deploying

Every push to `main` that touches the server, the auth server, the Mac app, the GPUI app or the
Android app is a release. The `Release` workflow publishes the server and the auth server for Linux,
the server, the client and the GPUI app for Macs, the GPUI app for Linux and Windows, and the Android
app (`Motile-android.apk`), as a GitHub release. Its
version is the first two numbers of `version` in `Cargo.toml` and the number of commits as the third
(`scripts/release-version.sh`), so set `version` only to change the first two. The installer and the
download button always fetch the latest release, and clients and servers compare their own version
with it to offer an update. The Mac app in a release is signed with the Developer ID certificate
and notarized, using the repository's `APPLE_*` secrets; running the `macOS` workflow by hand with
`sign` does the same without a release.

Every push and pull request that touches the Android app lints, tests and builds it (the `Android`
workflow). Its release is built by `apps/android/scripts/release.sh` with the release's version, the
commit count as its version code, and signed with the key in the `ANDROID_KEYSTORE`,
`ANDROID_KEYSTORE_PASSWORD` and `ANDROID_KEY_ALIAS` secrets. Every release must be signed with that
one key: the client only installs an update signed as it is, and Android refuses one that isn't.
Without the secrets, a release carries no Android app.

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
`macos`, `ios`, `android`, `apple`, `gpui`, `core` and `protocol`. Use `ci` for the workflows and `docs` for README.md and AGENTS.md.

The title should be concise. Description should explain the work in more detail (only if
required) while still being concise. Use simple language, do not try to sound smart.
