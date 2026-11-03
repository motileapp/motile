# The Gateway: Agent Gateway and MCP Gateway

The box the user runs is the **gateway**. It does two jobs, and each has its own settings page:

- **Agent Gateway**: the Claude Code and Codex accounts, how threads are placed on them, and
  the request log. This is today's Proxy page.
- **MCP Gateway**: the MCP servers it signs in to once, which every agent gets, with their tools
  and the calls agents made. This is today's Connectors page.

The gateway itself (add, replace, remove, connected or not, version) is a row at the top of both
pages, never the property of one. Nobody else runs it yet, so the rename is complete, code included,
with nothing kept for compatibility beyond the user's own data.

## Words

| Before | After |
| --- | --- |
| proxy (the box) | gateway |
| Settings → Proxy | Settings → Agent Gateway |
| Settings → Connectors | Settings → MCP Gateway |
| connector | MCP server |
| Proxy account (in the picker) | Gateway account |
| "Add a Proxy", "Replace Your Proxy", "Remove Your Proxy?" | "Add a Gateway", "Replace Your Gateway", "Remove Your Gateway?" |
| "Add Connector", "No connectors yet" | "Add MCP Server", "No MCP servers yet" |
| "Remove the “Linear” Connector?" | "Remove the “Linear” MCP Server?" |
| "Connected to Linear" | "Connected to Linear" (unchanged) |
| `motile-proxy` (binary and image) | `motile-gateway` |
| `apps/proxy` | `apps/gateway` |

Every sentence that says "your proxy" says "your gateway". "Gateway" alone is the box. "Agent
Gateway" and "MCP Gateway" are only the names of the two pages, never of a thing to set up.

## Settings

**Sections**: Agent Gateway (symbol `waypoints`) and MCP Gateway (symbol `plug`), next to each
other after Servers, in every client and in the iOS and Android lists. Search entries: `gateway`
(the box, on both pages), `gateway-accounts`, `gateway-log`, `mcp-servers`, `mcp-activity`.

**The gateway row** (`GatewayHeader`, written once per client and put at the top of both pages):
- With a gateway: its name and status ("Connected · version 0.1.770"), the error under it in
  warning, and the `⋯` menu with Replace with a New Gateway and Remove Gateway.
- Without one: "No gateway", "Runs on a machine that is always on", and `Add a Gateway`, which opens
  the add sheet both pages share.
- Under the row, a line saying what the page does with it:
  - Agent Gateway: "Holds your Claude Code and Codex accounts. Every server's Gateway accounts use
    them."
  - MCP Gateway: "Signs in to each MCP server once. Every agent on every server and account gets
    its tools."

**Agent Gateway** is today's Proxy page under the row: Claude Code Accounts, Codex Accounts, Log.
**MCP Gateway** is today's Connectors page under the row: MCP Servers (Add MCP Server, the cards),
Activity. When there is no gateway, each page shows only the row.

## Code

**Gateway** (`apps/proxy` → `apps/gateway`, crate `motile-gateway`):
- Modules: `proxy.rs` → `gateway.rs` (the `Gateway` state). `connectors/` → `mcp/`, whose
  passthrough is `mcp/passthrough.rs`.
- Types: `Connector` → `McpServer`.
- Data: on start, an old `proxy.sqlite` in the data folder is renamed to `gateway.sqlite`, so the
  user's accounts and MCP servers carry over. The volume stays at `/data`.
- Environment: `MOTILE_PROXY_PORT` → `MOTILE_GATEWAY_PORT`. `MOTILE_ENROLL_CODE` stays.
- `Dockerfile` → `ghcr.io/motileapp/motile-gateway`.

**Protocol** (`proxy_wire.rs` → `gateway_wire.rs`):
- `PROXY_ALPN` (`motile-proxy/1`) → `GATEWAY_ALPN` (`motile-gateway/1`).
- `ProxyRequest`/`ProxyReply`/`ProxyState`/`ProxyAccount` → `GatewayRequest`/…
- `Connectors` → `McpServers`, `AddConnector` → `AddMcpServer` and the rest alike, and
  `ProxyConnector`/`ConnectorTool`/`ConnectorCall` → `McpServer`/`McpTool`/`McpCall`.
- `DeviceKind::Proxy` → `DeviceKind::Gateway`, serialized `gateway`, with `proxy` still read.

**Auth server**:
- Migration `0006_gateway.sql` turns `kind = 'proxy'` into `'gateway'` and changes the check
  constraint.
- "Proxy" becomes "Gateway" in its names and comments. `apps/auth/src/e2e` covers enrolling a
  gateway as it covers a proxy today.

**Server**:
- `proxy.rs` → `gateway.rs` (`Tunnel`).
- Agents reach MCP servers at `/mcp/<slug>` instead of `/connectors/<slug>/mcp`.
- `MOTILE_PROXY_ADDR` → `MOTILE_GATEWAY_ADDR`, `MOTILE_PROXY_TOKEN` → `MOTILE_GATEWAY_TOKEN`.
- Agent account ids `proxy-claude`/`proxy-codex` (and their `:<pinned>` forms) → `gateway-claude`/
  `gateway-codex`, named "Gateway". A migration rewrites them where the server keeps them: its
  agent accounts, its threads' `agent_account`, its sessions. The copies of other servers' threads
  aren't touched: they are rewritten when their own server migrates and syncs again.

**Core**:
- `proxy.rs` → `gateway.rs`, `connectors.rs` → `mcp_servers.rs`.
- Commands `proxy_*` → `gateway_*`, `connector_*` → `mcp_server_*`, event `proxy` → `gateway`.
- The cached state key changes too, so a client fetches it again.
- The sign-in's return address is `motile://mcp` instead of `motile://connector`.

**Clients** (all four):
- Files and types renamed to match (`ProxySection` → `GatewaySection`, `Connectors*` → `McpServers*`,
  `ConnectorRow` → `McpServerRow`, and so on).
- `GatewayHeader` is extracted from today's Proxy page and put on both pages.
- Settings sections, search entries, `Removal` cases and texts, and the demo drivers' `proxy` and
  `connectors` steps → `gateway` and `mcp`.
- Android's manifest host `connector` → `mcp`.

**Web app**: device kind `gateway`; the section titled "Gateway"; the waypoints icon stays.

**Scripts and CI**:
- `release.yml`: binaries, artifacts, job `gateway-image`, paths `apps/gateway/**`.
- `dev-stack.sh`: `start_gateway`, `$DEV/gateway`, `gateway.key`, `GATEWAY_PORT`. A dev stack that
  already has `proxy/` is moved over once.
- `scripts/fake-agent`: "Proxy account" → "Gateway account".
- `scripts/fake-upstream`: unchanged, except its docstring.

**Docs**:
- AGENTS.md: the program table, the `apps/gateway` section, the server, core and client sections,
  the commit-message parts (`gateway` instead of `proxy`), and the dev stack paragraph.
- README.md: Agent Gateway and MCP Gateway in place of Proxy and Connectors.
- `plans/connectors.md` is dropped for this file.

## Order

Each step leaves the workspace building and its tests passing.

1. Protocol, auth (migration and e2e), gateway crate (`git mv apps/proxy apps/gateway`, data file
   rename), server (ids migration, routes, variables), core. Then `cargo fmt`, `clippy`, `test`.
2. Words and pages in the Apple clients, `GatewayHeader` on both pages. Check on the Mac and the iOS
   dev apps.
3. The same in GPUI and Android. Their checks; GPUI's dev app.
4. Web app, scripts, CI, docs. `pnpm -r lint typecheck build`.
5. Screenshots of both pages and the shared row, Mac and iOS.

## Moving over your own setup

Once it is released:

- Run `ghcr.io/motileapp/motile-gateway` with the same `/data` volume instead of `motile-proxy`.
  It renames `proxy.sqlite` itself and keeps its device key, so it stays linked with the same
  accounts and MCP servers.
- The auth server's migration turns the existing proxy device into the gateway.
- Servers rename their Proxy accounts to Gateway accounts on their next start.

Old and new can't talk across the rename (the connection name changes), so update the gateway
and the servers together.
