#if os(macOS)
import SwiftUI

/// Settings → MCP Gateway: the MCP servers the account's gateway signs in to, a card each with its tools,
/// and the calls agents made to them. The core follows the gateway while the page shows.
struct McpGatewaySettings: View {
    @Environment(AppStore.self) private var store
    @State private var adding = false
    @State private var sheet: McpServerSheet?
    @State private var removal: Removal?
    @State private var call: McpServerCallEntry?
    @State private var unfolded: Set<String> = []
    @State private var calls = McpServerCalls()

    var body: some View {
        let gateway = store.gateway
        VStack(alignment: .leading, spacing: SettingsGroupTitle.groupGap) {
            GatewayHeader(caption: GatewayText.mcpCaption)
            if gateway.exists {
                mcpServers(gateway)
            }
            if gateway.exists, gateway.mcpServers?.isEmpty == false {
                activity
            }
        }
        .onAppear { store.watchGateway(true) }
        .onDisappear { store.watchGateway(false) }
        .onChange(of: gateway.mcpServers != nil, initial: true) {
            guard store.gateway.mcpServers != nil else { return }
            calls.start(store)
        }
        .onChange(of: gateway.callsHead) { calls.refresh() }
        .confirmsRemoval($removal)
        .modal(isPresented: $adding) { McpServerAddSheet() }
        .modal(item: $sheet) { sheet in
            switch sheet.kind {
            case .tryTool: McpServerTrySheet(mcpServer: sheet.mcpServer)
            case .rename: McpServerRenameSheet(mcpServer: sheet.mcpServer)
            case .key: McpServerKeySheet(mcpServer: sheet.mcpServer)
            }
        }
        .modal(item: $call) { McpServerCallSheet(entry: $0.id) }
    }

    /// The way to add an MCP server and a card for each, or why there can't be any yet.
    @ViewBuilder private func mcpServers(_ gateway: GatewayState) -> some View {
        SettingsGroup("mcp-servers", "MCP Servers", carded: false) {
            if let mcpServers = gateway.mcpServers {
                ActionButton("Add MCP Server", icon: .plus, variant: .outline, size: .large) { adding = true }
                    .frame(maxWidth: .infinity, alignment: .leading)
                if mcpServers.isEmpty {
                    SettingsRow {
                        SettingsLabel(McpServerText.none, description: McpServerText.noneDetail, icon: .plug)
                    } trailing: {
                        EmptyView()
                    }
                    .card()
                }
                ForEach(mcpServers) { mcpServer in
                    card(mcpServer)
                }
            } else {
                SettingsRow {
                    SettingsLabel(
                        gateway.connection == .connected ? McpServerText.updateGateway : gateway.statusText,
                        description: gateway.connection == .connected ? gateway.version.map { "Your gateway runs version \($0)" } : gateway.error,
                        icon: .waypoints
                    )
                } trailing: {
                    if gateway.connection == .connecting {
                        Spinner().foregroundStyle(Color.themeMutedForeground)
                    }
                }
                .card()
            }
        }
    }

    private func card(_ mcpServer: McpServerRow) -> some View {
        let open = unfolded.contains(mcpServer.id)
        return VStack(spacing: 0) {
            SettingsRow {
                HStack(spacing: 10) {
                    McpServerTile(monogram: mcpServer.monogram, tint: mcpServer.tint)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(mcpServer.name)
                            .font(.ui(size: 13, weight: .medium))
                            .foregroundStyle(Color.themeForeground)
                        Text(mcpServer.statusText)
                            .font(.ui(size: 11.5))
                            .foregroundStyle(mcpServer.troubled ? Color.themeWarning : Color.themeMutedForeground)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                .opacity(.disabled, when: !mcpServer.enabled)
            } trailing: {
                if mcpServer.state == .signInAgain {
                    ActionButton("Sign In Again", variant: .outline, size: .large, pending: store.mcpServerSigningIn == mcpServer.id) {
                        store.signMcpServerInAgain(mcpServer)
                    }
                }
                Switch(isOn: Binding { mcpServer.enabled } set: { store.setMcpServer(mcpServer, enabled: $0) })
                ActionMenu(icon: .ellipsis, help: "What to do with \(mcpServer.name)", size: .large) {
                    McpServerActions(
                        mcpServer: mcpServer,
                        tryTool: { sheet = McpServerSheet(kind: .tryTool, mcpServer: mcpServer) },
                        rename: { sheet = McpServerSheet(kind: .rename, mcpServer: mcpServer) },
                        changeKey: { sheet = McpServerSheet(kind: .key, mcpServer: mcpServer) },
                        remove: { removal = .mcpServer(mcpServer) }
                    )
                }
            }
            if !mcpServer.tools.isEmpty {
                ThemeDivider()
                fold(mcpServer, open: open)
                if open {
                    tools(mcpServer)
                }
            }
        }
        .card()
    }

    /// How many tools it gives agents, which unfolds them.
    private func fold(_ mcpServer: McpServerRow, open: Bool) -> some View {
        Button {
            withAnimation(.snappy(duration: 0.2)) {
                if open { unfolded.remove(mcpServer.id) } else { unfolded.insert(mcpServer.id) }
            }
        } label: {
            HStack(spacing: 8) {
                Text(mcpServer.toolsText)
                    .font(.ui(size: 12.5))
                    .foregroundStyle(Color.themeMutedForeground)
                Spacer()
                Image(open ? .chevronDown : .chevronRight, size: .sm)
                    .foregroundStyle(Color.themeMutedForeground)
            }
            .padding(.horizontal, settingsInset)
            .frame(height: scaled(36))
            .contentShape(Rectangle())
        }
        .buttonStyle(.highlight(radius: 0))
        .opacity(.disabled, when: !mcpServer.enabled)
    }

    private func tools(_ mcpServer: McpServerRow) -> some View {
        VStack(spacing: 0) {
            ThemeDivider()
            HStack {
                Text("Agents call them as \(mcpServer.example)")
                    .font(.ui(size: 11.5, design: .monospaced))
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
                Spacer(minLength: 12)
                McpServerToolsActions(mcpServer: mcpServer)
            }
            .padding(.horizontal, settingsInset)
            .padding(.vertical, 8)
            ForEach(mcpServer.tools) { tool in
                ThemeDivider()
                McpServerToolRow(mcpServer: mcpServer, tool: tool)
                    .padding(.horizontal, settingsInset)
                    .padding(.vertical, 9)
            }
        }
        .opacity(.disabled, when: !mcpServer.enabled)
    }

    /// The calls agents made, newest first, all of them or only those that failed.
    private var activity: some View {
        SettingsGroup("mcp-activity", "Activity", carded: false) {
            VStack(spacing: 0) {
                Segmented([("All", false), ("Errors", true)], selection: $calls.errorsOnly)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, settingsInset)
                    .padding(.vertical, 10)
                if let empty = calls.emptyText {
                    ThemeDivider()
                    Text(empty)
                        .font(.ui(size: 12))
                        .foregroundStyle(Color.themeMutedForeground)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, settingsInset)
                        .padding(.vertical, 14)
                }
                LazyVStack(spacing: 0) {
                    ForEach(calls.rows) { row in
                        ThemeDivider()
                        Button { call = McpServerCallEntry(id: row.id) } label: {
                            McpServerCallRowView(row: row)
                                .padding(.horizontal, settingsInset)
                                .padding(.vertical, 10)
                        }
                        .buttonStyle(.highlight(radius: 0))
                    }
                    if calls.more || (calls.loading && calls.rows.isEmpty) {
                        ThemeDivider()
                        Spinner()
                            .foregroundStyle(Color.themeMutedForeground)
                            .frame(maxWidth: .infinity, minHeight: 44)
                            .task(id: calls.rows.count) { calls.loadMore() }
                    }
                }
            }
            .card()
        }
    }
}
#endif
