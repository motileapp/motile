## What is this?

Motile is an open-source command center for coding agents. The agents (Claude Code and Codex)
run on machines the user owns, called hosts. Native apps drive them: an app connects straight to
its hosts over [iroh](https://www.iroh.computer), which needs no open ports, and keeps a local
copy of every thread so it opens where it was left. There is no local mode; an app always talks
to a host.

Three programs make it up, plus the code the apps share:

| Program | Where it runs | What it does |
| --- | --- | --- |
| Auth server (`apps/auth`) | motile.app | Signs people in with Google, records which devices belong to an account, serves the installer |
| Host (`apps/server`, the `motile` binary) | The user's Linux machines | Runs the agents, stores threads in SQLite, serves the account's apps |
| Mac app (`apps/mac`) | The user's Mac | The interface |
| Core (`crates/core`) | Inside every app | Account, connections, sync, the local cache, rendering transcripts |

Every device is an ed25519 key, which is also its iroh address. The auth server only says which
keys belong to one account; it never sees a thread. A host accepts connections only from its
account's apps. README.md covers how a user sets things up.

Production is the `Motile` project on Unbind: the `Motile` service (the auth server) and a
`Postgres` database, at https://motile.app.

## Repo Structure:

### crates/protocol

What the three programs agree on.

- `wire.rs` is every message between an app and a host. Changing it changes both sides; bump
  `PROTOCOL_VERSION` when an old app or host could no longer understand the other.
- `auth_api.rs` and `auth_client.rs` are the auth server's JSON and the client for it.
- `identity.rs` is the device key and request signing: a linked device signs its requests to the
  auth server instead of holding a token.

### apps/auth (Rust, Axum, sqlx on Postgres)

- `sign_in.rs` is the browser's side of a sign-in: `/auth/start`, Google, and back to the app at
  `motile://auth` with a one-time code. `google.rs` is the OIDC exchange.
- `api.rs` is what apps and hosts call: exchanging the code for a linked device, install tokens
  (`/api/enroll-tokens`, `/api/enroll`), `/api/me`, removing devices.
- `pages.rs` serves the landing page, `/install` (`assets/install.sh`) and `/download/<file>`,
  which redirects to the latest GitHub release.
- `migrations/` is the schema. Expired sign-ins and tokens are deleted every minute.
- `e2e/` runs the real router on a fresh database per test, with a fake Google. `e2e/whole.rs`
  runs all three programs together.
- `DEV_LOGIN=1` lets anyone sign in as anyone without Google. It exists for tests, the Mac demo
  and local work, and must never be set in production.

### apps/server (the host)

- `hub.rs` is the live state of every thread. A turn is one run of the agent's CLI; its events
  are applied there, saved, and sent to every app that has the thread open.
- `agents/` builds the command for a turn and parses its output: `claude.rs` for
  `claude -p --output-format stream-json`, `codex.rs` for `codex exec --json`. Both become the
  same `AgentEvent`s. `models.rs` lists Claude's models by hand; Codex's are read from its cache.
- `store.rs` is SQLite. Every item has a position (`seq`) and the revision that last changed it
  (`rev`); an app asks for what changed after the revision it has.
- `title.rs` generates thread titles with the thread's own agent.
- `setup.rs` is what the installer runs: it checks for agents, links the host with the install
  token, and installs the systemd unit. The unit is a system unit that runs as the installing
  user; for root it sets `IS_SANDBOX=1`, without which Claude Code refuses full access.
- `access.rs` asks the auth server which apps belong to the account, and caches the answer.
- `tests/e2e.rs` runs the host against `scripts/fake-agent` over real iroh connections.

### crates/core

Built as a static library for the apps (`ffi.rs`: JSON commands in, JSON events out) and as a
Rust library for tests.

- `core.rs` is one loop that owns all state. Commands from the app and events from the hosts
  arrive on one channel; anything that waits on the network runs in its own task.
- `link.rs` keeps one host connected and follows its thread list and the open threads.
- `cache.rs` is the app's SQLite copy of its hosts' threads.
- `render/` turns transcripts into rows ready to draw: `markdown.rs` (text with style runs, in
  UTF-16 offsets), `highlight.rs` (syntect; streaming code is highlighted incrementally) and
  `rows.rs` (the row list and the splices sent to the app).
- `api.rs` is the JSON the app and the core exchange. `examples/drive.rs` drives the core from a
  terminal.

### apps/mac (Swift: SwiftUI, with AppKit for the transcript)

- `Core/CoreBridge.swift` calls the Rust core; `Core/AppStore.swift` is all the state the views
  show. Events are decoded off the main thread.
- `Views/Transcript/` is the transcript: `TranscriptView.swift` only keeps views for the rows on
  screen, `RowViews.swift` are the rows, `Rows.swift` builds their text off the main thread.
- `Views/Sidebar`, `Views/Thread`, `Views/Composer` and `Views/Onboarding` are SwiftUI.
- `Demo/DemoDriver.swift` walks the app through a scripted demo; `scripts/ci-demo.sh` runs it
  in CI against a real auth server and host and collects screenshots and `checks.txt`.

## General Rules:

- Keep it simple. Do not overcomplicate things.
- The UI must never stall. Nothing slow runs on the main thread: parsing, highlighting, decoding
  and text layout preparation happen in the core or on a background queue, and the transcript
  only ever builds what is on screen. If a change touches the transcript, run the Mac workflow
  and read the stall numbers in `checks.txt`.
- The auth server is security-critical. The sign-in code is only ever sent to `motile://auth`,
  works once, and only with the secret that started the sign-in. Codes and tokens are stored
  hashed. Anything that changes this needs a test in `apps/auth/src/e2e`.
- A host runs agents with full access to its machine. It must only ever accept devices of its
  own account.
- Rendering logic belongs in `crates/core`, not in an app, so that every future app (iOS,
  Android, Windows, Linux) gets it.
- Do not leave paragraphs of comments on top of the code. You should try to avoid them as much
  as possible with understandable function names and code. If they are necessary even then, make
  them concise. Remove such comments when you come by them in the codebase. Comments should
  always move with code, not be left behind.
- Use guard statement patterns in any code you write.
- Do not edit generated code: `Cargo.lock`. Never edit an applied migration in
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

Needs Rust stable and Docker (for Postgres). The Mac app needs Xcode 16 or later.

    docker compose up -d                              # Postgres on localhost:5435
    cargo run -p motile-auth                          # with the variables from .env.example exported
    cargo run -p motile-server -- run                 # a host, once linked with `motile setup <token>`
    cargo run -p motile-core --example drive          # the core, driven from a terminal
    apps/mac/scripts/build-app.sh --open              # on a Mac

Checks (`cargo test` needs the compose Postgres; it creates a throwaway database per test):

    export DATABASE_URL=postgres://motile:motile@localhost:5435/motile
    cargo fmt --all && cargo clippy --workspace --all-targets && cargo test --workspace

The Mac app can't be built on Linux. The `Mac` workflow builds it on every push that touches it,
runs the demo and uploads the app and the screenshots:

    gh run watch                                      # then
    gh run download --name screenshots

`scripts/fake-agent` stands in for Claude Code and Codex in the tests and the demo. It replays
the recorded output in `fixtures/` or makes up a turn, depending on the prompt.

## Releasing and deploying

Pushing a tag `v*` runs the `Release` workflow, which publishes the host and the auth server for
Linux and the Mac app as a GitHub release. The installer and the download button always fetch
the latest release.

The auth server on Unbind is an `alpine` image whose run command downloads `motile-auth` from
the latest release, so deploying it is: tag a release, then restart the `Motile` service.
(Unbind's GitHub app isn't installed on the `motileapp` organisation; if it is one day, the
service can build from the `Dockerfile` instead.)

## Commit Messages

A short imperative sentence describing the change, no prefixes:

    Refuse a sign-in code that was started by another app
    Show the host's round-trip time in the sidebar

The title should be concise. Description should explain the work in more detail (only if
required) while still being concise. Use simple language, do not try to sound smart.
